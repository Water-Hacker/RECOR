//! Composition root for the RÉCOR Verification Engine.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use recor_verification_engine::api::{AppState, OidcVerifier};
use recor_verification_engine::application::{
    stages::{
        AdverseMediaStage, AdverseMediaStub, BunecNameResolver, CrossSourceStub,
        CrossSourceTriangulationStage,
        IdentityAuthenticationStage, NameResolver, PatternDetectionStage,
        PatternDetectionStub, PepStage, PepStub, SanctionsStage, SanctionsStub,
        SchemaValidationStage,
    },
    GetVerificationUseCase, PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::config::Config;
use recor_verification_engine::domain::{LaneThresholds, Stage};
use recor_verification_engine::infrastructure::{
    KafkaConsumer, OutboxAdminStore, PostgresIcijRepository, PostgresMockBunec,
    PostgresPepAdapter, PostgresSanctionsAdapter, PostgresVerificationRepository,
    VerificationOutboxRelay, VerificationOutboxRetention, WritebackSubscriber,
};
use recor_verification_engine::infrastructure::retention::warn_if_misconfigured;

#[tokio::main]
async fn main() -> Result<()> {
    // OPS-4: Vault bridge — see services/declaration/src/main.rs for
    // the rationale and the equivalent comment. When VAULT_ADDR is
    // set, fetch the V-engine's secrets from Vault and inject them
    // into env before Config::from_env() runs. When empty, env-only
    // mode with a startup warn!.
    let vault_paths: &[(&str, &[(&str, &str)])] = &[
        (
            "recor/verification-engine/database",
            &[("DATABASE_URL", "DATABASE_URL")],
        ),
        (
            "recor/verification-engine/inbound",
            &[
                ("INBOUND_HMAC_SECRET", "INBOUND_HMAC_SECRET"),
                ("INBOUND_HMAC_SECRET_OLD", "INBOUND_HMAC_SECRET_OLD"),
            ],
        ),
        (
            "recor/verification-engine/writeback",
            &[("WRITEBACK_HMAC_SECRET", "WRITEBACK_HMAC_SECRET")],
        ),
        (
            "recor/verification-engine/oidc",
            &[
                ("OIDC_ISSUER_URL", "OIDC_ISSUER_URL"),
                ("OIDC_AUDIENCE", "OIDC_AUDIENCE"),
            ],
        ),
        (
            "recor/verification-engine/observability",
            &[("LOG_REDACTION_KEY", "LOG_REDACTION_KEY")],
        ),
    ];
    recor_vault_client::populate_from_vault(vault_paths)
        .await
        .map_err(|e| anyhow::anyhow!("Vault secret loading failed (D14 fail-closed): {e}"))?;

    let cfg = Config::from_env().context("loading configuration")?;
    let _guard = recor_verification_engine::observability::init(&cfg)
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;

    info!(
        service = %cfg.service_name,
        env = %cfg.environment,
        bind = %cfg.bind_addr,
        "recor-verification-engine starting"
    );

    let pool = PgPoolOptions::new()
        .max_connections(cfg.db_pool_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(cfg.database_url.expose_secret())
        .await
        .context("connecting to Postgres")?;

    let repository = Arc::new(PostgresVerificationRepository::new(pool.clone()));
    repository.run_migrations().await.context("migrations")?;
    info!("migrations applied");

    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));

    // TODO-015 — BUNEC adapter selection. `mock` (default) wires the
    // Postgres-backed mock seeded by integration smoke tests. `real`
    // wires the REST adapter at `BUNEC_BASE_URL`. When the operator
    // declares `real` but the cross-org agreement is not yet in
    // place (empty `BUNEC_BASE_URL` or empty API key), the service
    // refuses to start — D14 fail-closed beats "silently fall back
    // to the mock and pretend BUNEC is responding".
    use recor_verification_engine::application::port::BunecAdapter;
    let bunec: Arc<dyn BunecAdapter> = match cfg.bunec_adapter_kind.as_str() {
        "real" => {
            use recor_verification_engine::infrastructure::bunec_real::{
                BunecFailPolicy, BunecRealConfig, RealBunecAdapter,
            };
            use secrecy::ExposeSecret;
            if cfg.bunec_base_url.trim().is_empty()
                || cfg.bunec_api_key.expose_secret().is_empty()
            {
                anyhow::bail!(
                    "BUNEC_ADAPTER_KIND=real but BUNEC_BASE_URL or BUNEC_API_KEY is empty (TODO-015: cross-org agreement not in place)"
                );
            }
            let real_cfg = BunecRealConfig {
                base_url: cfg.bunec_base_url.clone(),
                api_key: cfg.bunec_api_key.clone(),
                request_timeout: std::time::Duration::from_secs(cfg.bunec_timeout_secs),
                retry_attempts: cfg.bunec_retry_attempts,
                retry_base_backoff: std::time::Duration::from_millis(
                    cfg.bunec_retry_backoff_ms,
                ),
                breaker_consecutive_failures: cfg.bunec_breaker_consecutive_failures,
                breaker_half_open_after: std::time::Duration::from_secs(
                    cfg.bunec_breaker_half_open_secs,
                ),
                fail_policy: BunecFailPolicy::parse(
                    &cfg.bunec_fail_policy,
                    &cfg.environment,
                ),
            };
            info!(
                base_url = %cfg.bunec_base_url,
                fail_policy = ?real_cfg.fail_policy,
                "BUNEC adapter: REAL (TODO-015 production path)"
            );
            Arc::new(
                RealBunecAdapter::new(real_cfg)
                    .context("constructing real BUNEC adapter")?,
            )
        }
        "mock" | "" => {
            info!(
                "BUNEC adapter: MOCK (TODO-015 — switch to real when cross-org agreement lands)"
            );
            Arc::new(PostgresMockBunec::new(pool.clone()))
        }
        other => {
            anyhow::bail!(
                "BUNEC_ADAPTER_KIND must be `real` or `mock` (got `{other}`)"
            );
        }
    };

    // FIND-009 closure. Each Stage 3..6 starts as a stub and is
    // replaced with the real implementation when the corresponding
    // `enable_real_*` config flag is set. Stage 7 (cross-source)
    // has no real implementation in the crate yet; it stays stub.
    //
    // The real stages need a `NameResolver` (sanctions, PEP, adverse
    // media) AND/OR a Postgres pool (patterns) AND/OR the Inference
    // Gateway (adverse media). We construct the resolver + adapters
    // up front so the per-stage switch can drop them in cheaply.
    let name_resolver: Arc<dyn NameResolver> =
        Arc::new(BunecNameResolver::new(bunec.clone()));

    let sanctions_stage: Arc<dyn Stage> = if cfg.enable_real_sanctions {
        info!("Stage 3 (sanctions): real implementation enabled");
        let adapter = Arc::new(PostgresSanctionsAdapter::new(pool.clone()));
        Arc::new(SanctionsStage::new(adapter, name_resolver.clone()))
    } else {
        info!("Stage 3 (sanctions): stub (set ENABLE_REAL_SANCTIONS=true to enable the real path)");
        Arc::new(SanctionsStub::new())
    };

    let pep_stage: Arc<dyn Stage> = if cfg.enable_real_pep {
        info!("Stage 4 (PEP): real implementation enabled");
        let adapter = Arc::new(PostgresPepAdapter::new(pool.clone()));
        Arc::new(PepStage::new(adapter, name_resolver.clone()))
    } else {
        info!("Stage 4 (PEP): stub (set ENABLE_REAL_PEP=true to enable the real path)");
        Arc::new(PepStub::new())
    };

    let adverse_media_stage: Arc<dyn Stage> = if cfg.enable_real_adverse_media {
        info!("Stage 5 (adverse media): real implementation enabled");
        let icij = Arc::new(PostgresIcijRepository::new(pool.clone()));
        let gateway = Arc::new(
            recor_inference_gateway::InferenceGateway::new(
                recor_inference_gateway::GatewayConfig::from_env(),
            )
            .context("inference gateway init failed (Stage 5 real path)")?,
        );
        Arc::new(AdverseMediaStage::new(icij, name_resolver.clone(), gateway))
    } else {
        info!("Stage 5 (adverse media): stub (set ENABLE_REAL_ADVERSE_MEDIA=true to enable the real path)");
        Arc::new(AdverseMediaStub::new())
    };

    let pattern_stage: Arc<dyn Stage> = if cfg.enable_real_patterns {
        info!("Stage 6 (patterns): real implementation enabled");
        Arc::new(PatternDetectionStage::new(pool.clone()))
    } else {
        info!("Stage 6 (patterns): stub (set ENABLE_REAL_PATTERNS=true to enable the real path)");
        Arc::new(PatternDetectionStub::new())
    };

    // TODO-013 — Stage 7 selection (cross-source triangulation).
    // Real path consumes upstream Stage 3-6 outcomes + the
    // declaration's structural claims; ADR-0014 documents the
    // decision rules.
    let stage7: Arc<dyn Stage> = if cfg.enable_real_stage7 {
        info!("Stage 7 (cross-source): real implementation enabled");
        Arc::new(CrossSourceTriangulationStage::new())
    } else {
        info!(
            "Stage 7 (cross-source): stub (set ENABLE_REAL_STAGE7=true to enable the real path)"
        );
        Arc::new(CrossSourceStub::new())
    };

    let stages: Vec<Arc<dyn Stage>> = vec![
        Arc::new(SchemaValidationStage::new()),
        Arc::new(IdentityAuthenticationStage::new(bunec.clone())),
        sanctions_stage,
        pep_stage,
        adverse_media_stage,
        pattern_stage,
        stage7,
    ];

    let orchestrator = Arc::new(PipelineOrchestrator::new(stages, LaneThresholds::default()));
    let submit = Arc::new(SubmitVerificationUseCase::new(orchestrator.clone(), repository.clone()));
    let submit_for_kafka = submit.clone();
    let get = Arc::new(GetVerificationUseCase::new(repository.clone()));

    // OIDC verifier — discovered at startup with JWKS pre-warm.
    // `None` in dev when OIDC_ISSUER_URL is unset; production refuses
    // at config load.
    let oidc = if cfg.oidc_issuer_url.is_empty() {
        info!("OIDC verifier disabled (dev mode — OIDC_ISSUER_URL unset)");
        None
    } else {
        use recor_verification_engine::api::oidc::OidcVerifierBuilder;
        let builder = OidcVerifierBuilder::new(
            cfg.oidc_issuer_url.clone(),
            cfg.oidc_audience.clone(),
        )
        .subject_claim(cfg.oidc_subject_claim.clone());
        let v = OidcVerifier::discover_with_builder(builder)
            .await
            .context("OIDC discovery against configured issuer")?;
        info!(
            issuer = %cfg.oidc_issuer_url,
            audience = %cfg.oidc_audience,
            subject_claim = %cfg.oidc_subject_claim,
            "OIDC verifier ready (JWKS pre-warmed)"
        );
        Some(v)
    };

    // OBS-1: build the per-service Prometheus registry once at startup.
    let metrics = recor_verification_engine::metrics::Metrics::new()
        .map_err(|e| anyhow::anyhow!("prometheus registry init failed: {e}"))?;
    info!("prometheus metrics registry initialised");

    // R-LOOP-3 — SPIFFE/mTLS bootstrap. Same shape as the declaration
    // service: if the operator asked for mTLS we refuse to start
    // unless the Workload API hands us a valid SVID (D14 fail-closed
    // / D7 no-workarounds).
    let spiffe_metrics = std::sync::Arc::new(
        recor_spiffe::SpiffeMetrics::register(&metrics.registry)
            .map_err(|e| anyhow::anyhow!("spiffe metrics register failed: {e}"))?,
    );
    let spiffe_client = if cfg.mtls_enabled() {
        info!(
            socket = %cfg.spiffe_socket,
            self_id = %cfg.spiffe_id_self,
            peer_id = %cfg.spiffe_id_peer,
            transport = %cfg.auth_transport,
            "AUTH_TRANSPORT requires SPIFFE — bootstrapping Workload API client"
        );
        let api = std::sync::Arc::new(
            recor_spiffe::HttpWorkloadApi::new(cfg.spiffe_socket.clone()),
        );
        let client = std::sync::Arc::new(recor_spiffe::SpiffeClient::new(
            api,
            Some(spiffe_metrics.clone()),
        ));
        client
            .bootstrap(&cfg.spiffe_id_self)
            .await
            .context("SPIFFE Workload API bootstrap failed — refusing to start under AUTH_TRANSPORT=mtls (D14 fail-closed)")?;
        info!("SPIFFE SVID + trust bundle fetched");
        // TODO(R-LOOP-3-followup): swap axum::serve for axum-server +
        // rustls::ServerConfig built from spiffe_client + add a tower
        // middleware that extracts the peer SPIFFE ID and enforces
        // cfg.spiffe_id_peer via recor_spiffe::enforce_peer_id.
        Some(client)
    } else {
        info!(
            transport = %cfg.auth_transport,
            "AUTH_TRANSPORT=hmac — SPIFFE not bootstrapped"
        );
        None
    };
    let _spiffe = spiffe_client;

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        repository,
        outbox_admin,
        is_dev: cfg.is_dev(),
        oidc,
        metrics: metrics.clone(),
        admin_principals: Arc::new(cfg.admin_principals_list().into_iter().collect()),
    };

    // FIND-007: when METRICS_BIND_ADDR is set, /metrics is bound on a
    // separate listener and a NetworkPolicy restricts that port to the
    // Prometheus scraper. Empty preserves the current single-listener
    // posture (dev / single-port deployments).
    let expose_metrics_on_main = cfg.metrics_bind_addr.is_empty();
    let router =
        recor_verification_engine::api::router(app_state, &cfg, expose_metrics_on_main);
    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing bind addr")?;
    let listener = TcpListener::bind(addr).await.context("binding")?;
    info!(%addr, "listening");

    // Cancellation token shared with the writeback relay so shutdown
    // is coordinated with the HTTP server.
    let cancel = CancellationToken::new();

    // FIND-007: separate metrics listener. Spawned conditionally so a
    // misconfigured METRICS_BIND_ADDR doesn't crash the main HTTP path.
    let metrics_handle = if !cfg.metrics_bind_addr.is_empty() {
        let m_addr: SocketAddr = cfg
            .metrics_bind_addr
            .parse()
            .context("parsing metrics_bind_addr")?;
        let m_listener = TcpListener::bind(m_addr)
            .await
            .with_context(|| format!("binding metrics listener {m_addr}"))?;
        let m_router =
            recor_verification_engine::api::metrics_only_router(metrics.clone());
        let cancel_metrics = cancel.clone();
        info!(addr = %m_addr, "metrics listener bound (FIND-007 separate-port posture)");
        Some(tokio::spawn(async move {
            if let Err(e) = axum::serve(m_listener, m_router)
                .with_graceful_shutdown(async move {
                    cancel_metrics.cancelled().await;
                })
                .await
            {
                tracing::error!(error = ?e, "metrics listener error");
            }
        }))
    } else {
        info!("metrics listener disabled (METRICS_BIND_ADDR not set) — /metrics is on the main listener");
        None
    };

    // Writeback relay — optional. Enabled when WRITEBACK_URL is set.
    // When disabled, verification_outbox rows accumulate undispatched.
    let relay_handle = if !cfg.writeback_url.is_empty() {
        let subscriber = WritebackSubscriber {
            name: "declaration-service".to_string(),
            url: cfg.writeback_url.clone(),
            hmac_secret: cfg.writeback_hmac_secret.expose_secret().to_string(),
        };
        let relay = VerificationOutboxRelay::new(pool.clone(), subscriber)
            .with_poll_interval(std::time::Duration::from_secs(
                cfg.writeback_poll_interval_seconds,
            ))
            .with_max_attempts(cfg.writeback_max_attempts);
        let cancel_relay = cancel.clone();
        info!(
            url = %cfg.writeback_url,
            poll_interval_s = cfg.writeback_poll_interval_seconds,
            max_attempts = cfg.writeback_max_attempts,
            "writeback relay enabled"
        );
        Some(tokio::spawn(async move {
            relay.run(cancel_relay).await;
        }))
    } else {
        info!("writeback relay disabled (WRITEBACK_URL not set)");
        None
    };

    // COMP-2 — verification outbox retention worker. Same shape as the
    // declaration service: spawned unconditionally but a 0-day setting
    // disables pruning at runtime (safe default for tests).
    warn_if_misconfigured(
        cfg.outbox_retention_days,
        cfg.outbox_retention_interval_seconds,
    );
    let retention = VerificationOutboxRetention::new(pool.clone())
        .with_retention_days(cfg.outbox_retention_days)
        .with_interval(std::time::Duration::from_secs(
            cfg.outbox_retention_interval_seconds,
        ))
        .with_metrics(metrics.clone());
    info!(
        retention_days = cfg.outbox_retention_days,
        interval_s = cfg.outbox_retention_interval_seconds,
        "verification outbox retention worker spawning"
    );
    let cancel_retention = cancel.clone();
    let retention_handle = tokio::spawn(async move {
        retention.run(cancel_retention).await;
    });

    // R-LOOP-2 — Kafka consumer. Enabled when KAFKA_BROKERS is set AND
    // VERIFICATION_TRANSPORT == "kafka". The HTTP `/v1/internal/
    // declaration-events` webhook continues to handle inbound from the
    // declaration's HTTP outbox-relay regardless of this flag — both
    // transports may be active during the cutover. The use case is
    // idempotent on declaration_id (see submit_verification.rs), so
    // a duplicate delivery is absorbed without double-applying state.
    let kafka_consumer_handle = if !cfg.kafka_brokers.is_empty()
        && cfg.verification_transport == "kafka"
    {
        match KafkaConsumer::build_consumer(&cfg.kafka_brokers, &cfg.kafka_consumer_group) {
            Ok(consumer_client) => {
                let consumer = KafkaConsumer::new(
                    consumer_client,
                    cfg.kafka_declaration_topic.clone(),
                    pool.clone(),
                    submit_for_kafka,
                )
                .with_metrics(metrics.clone());
                let cancel_consumer = cancel.clone();
                info!(
                    brokers = %cfg.kafka_brokers,
                    group_id = %cfg.kafka_consumer_group,
                    topic = %cfg.kafka_declaration_topic,
                    "kafka consumer enabled (R-LOOP-2)"
                );
                Some(tokio::spawn(async move {
                    consumer.run(cancel_consumer).await;
                }))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "kafka consumer build failed — continuing with HTTP webhook only"
                );
                None
            }
        }
    } else {
        info!(
            kafka_brokers_set = !cfg.kafka_brokers.is_empty(),
            verification_transport = %cfg.verification_transport,
            "kafka consumer disabled (R-LOOP-2 inactive)"
        );
        None
    };

    let cancel_serve = cancel.clone();
    let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
        shutdown_signal().await;
        cancel_serve.cancel();
    });

    if let Err(e) = serve.await {
        error!(error = ?e, "server error");
        cancel.cancel();
        return Err(anyhow::anyhow!(e));
    }

    if let Some(h) = relay_handle {
        let _ = h.await;
    }
    if let Some(h) = kafka_consumer_handle {
        let _ = h.await;
    }
    if let Some(h) = metrics_handle {
        let _ = h.await;
    }
    let _ = retention_handle.await;

    info!("recor-verification-engine stopped");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM");
    let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT");
    tokio::select! {
        _ = sigterm.recv() => info!("SIGTERM received"),
        _ = sigint.recv() => info!("SIGINT received"),
    }
}
