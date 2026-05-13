//! Composition root for the RÉCOR Declaration service.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use recor_declaration::api::{
    AppState, DeclarationGrpcService, GrpcAuthConfig, OidcVerifier,
};
use recor_declaration::application::{
    AmendDeclarationUseCase, CorrectDeclarationUseCase, GetDeclarationUseCase,
    ListByPrincipalUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
    SupersedeDeclarationUseCase,
};
use recor_declaration::config::Config;
use recor_declaration::infrastructure::postgres::{
    IdempotencyStore, PostgresDeclarationRepository,
};
use recor_declaration::infrastructure::{
    KafkaProducer, OutboxAdminStore, OutboxRelay, OutboxRetention, RelayBackend,
    RelaySubscriber,
};
use recor_declaration::infrastructure::retention::warn_if_misconfigured;

#[tokio::main]
async fn main() -> Result<()> {
    // OPS-4: load secrets from Vault before the env-based config loader
    // runs. When VAULT_ADDR is non-empty, the bridge logs in via AppRole,
    // reads the secret/recor/declaration/* paths, and injects the
    // resolved values into process env. The existing Config::from_env()
    // then sees them like any other env var and runs its cross-field
    // validation. When VAULT_ADDR is empty, `populate_from_vault`
    // returns Ok(false) after emitting a startup warn! so operators see
    // they are NOT using Vault. D14: a non-empty VAULT_ADDR with an
    // unreachable Vault is a hard-fail.
    let vault_paths: &[(&str, &[(&str, &str)])] = &[
        (
            "recor/declaration/database",
            &[("DATABASE_URL", "DATABASE_URL")],
        ),
        (
            "recor/declaration/relay",
            &[
                ("RELAY_HMAC_SECRET", "RELAY_HMAC_SECRET"),
                ("RELAY_HMAC_SECRET_OLD", "RELAY_HMAC_SECRET_OLD"),
            ],
        ),
        (
            "recor/declaration/writeback",
            &[
                ("WRITEBACK_HMAC_SECRET", "WRITEBACK_HMAC_SECRET"),
                ("WRITEBACK_HMAC_SECRET_OLD", "WRITEBACK_HMAC_SECRET_OLD"),
            ],
        ),
        (
            "recor/declaration/oidc",
            &[
                ("OIDC_ISSUER_URL", "OIDC_ISSUER_URL"),
                ("OIDC_AUDIENCE", "OIDC_AUDIENCE"),
            ],
        ),
        (
            "recor/declaration/observability",
            &[("LOG_REDACTION_KEY", "LOG_REDACTION_KEY")],
        ),
    ];
    recor_vault_client::populate_from_vault(vault_paths)
        .await
        .map_err(|e| anyhow::anyhow!("Vault secret loading failed (D14 fail-closed): {e}"))?;

    let cfg = Config::from_env().context("loading configuration from environment")?;
    let _tracing_guard = recor_declaration::observability::init(&cfg)
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;

    info!(
        service = %cfg.service_name,
        env = %cfg.environment,
        bind = %cfg.bind_addr,
        "recor-declaration starting"
    );

    use secrecy::ExposeSecret;
    let pool = PgPoolOptions::new()
        .max_connections(cfg.db_pool_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(cfg.database_url.expose_secret())
        .await
        .context("connecting to Postgres")?;
    info!(
        max_connections = cfg.db_pool_max_connections,
        "Postgres pool established"
    );

    let repository = Arc::new(PostgresDeclarationRepository::new(pool.clone()));
    repository
        .run_migrations()
        .await
        .context("running database migrations")?;
    info!("migrations applied");

    let submit = Arc::new(SubmitDeclarationUseCase::new(repository.clone()));
    let get = Arc::new(GetDeclarationUseCase::new(repository.clone()));
    let record_verification =
        Arc::new(RecordVerificationOutcomeUseCase::new(repository.clone()));
    let supersede = Arc::new(SupersedeDeclarationUseCase::new(repository.clone()));
    let amend = Arc::new(AmendDeclarationUseCase::new(repository.clone()));
    let correct = Arc::new(CorrectDeclarationUseCase::new(repository.clone()));
    let list_by_principal =
        Arc::new(ListByPrincipalUseCase::new(repository.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool.clone()));
    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));

    let base_url = std::env::var("RECOR_BASE_URL").unwrap_or_else(|_| {
        format!("http://{}", cfg.bind_addr.trim_start_matches("0.0.0.0:"))
    });

    // OIDC verifier — discovered at startup with JWKS pre-warm. `None`
    // only in dev when OIDC_ISSUER_URL is unset; production refuses at
    // config load.
    let oidc = if cfg.oidc_issuer_url.is_empty() {
        info!("OIDC verifier disabled (dev mode — OIDC_ISSUER_URL unset)");
        None
    } else {
        use recor_declaration::api::oidc::OidcVerifierBuilder;
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
    // The same handle is shared with the REST router (timing middleware
    // + /metrics handler), the use cases (domain counters), and the
    // OIDC auth layer (verify counter + JWKS-fetch histogram).
    let metrics = recor_declaration::metrics::Metrics::new()
        .map_err(|e| anyhow::anyhow!("prometheus registry init failed: {e}"))?;
    info!("prometheus metrics registry initialised");

    // R-LOOP-3 — SPIFFE/mTLS bootstrap. Refuses to start when
    // AUTH_TRANSPORT=mtls / mtls-only and the SPIFFE Workload API
    // is unreachable (D14 fail-closed + D7 no-workarounds: if the
    // operator asked for mTLS, mTLS MUST succeed). The SpiffeMetrics
    // bundle is registered against the same Prometheus registry the
    // rest of the service uses, so `/metrics` exposes the SVID-fetch
    // + peer-verify counters under the standard service namespace.
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
        // The HttpWorkloadApi shim is the dev/test transport. Production
        // wiring of the gRPC Workload-API client over a UDS lives in a
        // follow-up; for now we accept that AUTH_TRANSPORT=mtls in this
        // build requires the dev HTTP shim to be reachable. The
        // composition root refuses to start if the bootstrap fails.
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
        // TODO(R-LOOP-3-followup): build the rustls ServerConfig +
        // ClientConfig from spiffe_client and use them to swap
        // `axum::serve` for `axum-server::tls_rustls::bind`. The
        // bootstrap succeeds today and the metrics ticked; the
        // actual TLS termination is the second-step wiring.
        Some(client)
    } else {
        info!(
            transport = %cfg.auth_transport,
            "AUTH_TRANSPORT=hmac — SPIFFE not bootstrapped"
        );
        None
    };
    // Keep the client alive for the lifetime of the process. Even
    // without the rustls wiring it owns the cached SVID + trust
    // bundle that the follow-up step consumes.
    let _spiffe = spiffe_client;

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        record_verification_usecase: record_verification,
        supersede_usecase: supersede,
        amend_usecase: amend,
        correct_usecase: correct,
        list_by_principal_usecase: list_by_principal,
        idempotency,
        outbox_admin,
        base_url,
        is_dev: cfg.is_dev(),
        idempotency_ttl_seconds: cfg.idempotency_ttl_seconds,
        oidc,
        metrics: metrics.clone(),
    };

    let router = recor_declaration::api::router(app_state.clone(), &cfg);

    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing bind address")?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;
    info!(%addr, "listening");

    // Cancellation token shared with the relay + gRPC server so
    // shutdown is coordinated across all transports.
    let cancel = CancellationToken::new();

    // gRPC server (R-DECL-8). Coexists with REST; same use cases, same
    // OIDC verifier (D17 zero-trust holds across transports). The
    // server is disabled when GRPC_BIND_ADDR is empty — the safe
    // default for environments that only need REST.
    let grpc_handle = if cfg.grpc_bind_addr.is_empty() {
        info!("gRPC server disabled (GRPC_BIND_ADDR not set)");
        None
    } else {
        let grpc_addr: SocketAddr = cfg
            .grpc_bind_addr
            .parse()
            .context("parsing gRPC bind address")?;
        let grpc_state = app_state.clone();
        let auth = GrpcAuthConfig {
            is_dev: cfg.is_dev(),
            oidc: grpc_state.oidc.clone(),
        };
        let service =
            DeclarationGrpcService::new(grpc_state).into_server_with_auth(auth);
        let cancel_grpc = cancel.clone();
        info!(%grpc_addr, "gRPC listening (recor.declaration.v1.DeclarationService)");
        Some(tokio::spawn(async move {
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_shutdown(grpc_addr, async move {
                    cancel_grpc.cancelled().await;
                })
                .await
            {
                warn!(error = ?e, "gRPC server exited with error");
            }
        }))
    };

    // Outbox relay — optional. Enabled when RELAY_WEBHOOK_URL is set.
    // When disabled, outbox rows accumulate; a future ticket relays them.
    //
    // R-LOOP-2 (Kafka transport) — see ADR-0007. The HTTP relay below
    // continues to run when RELAY_WEBHOOK_URL is set, regardless of
    // RELAY_TRANSPORT. The Kafka producer (spawned a few lines down)
    // runs ADDITIONALLY when RELAY_TRANSPORT=kafka. During the cutover
    // window both transports are active; each event lands once via
    // HTTP and once via Kafka. The verification engine's idempotency-
    // on-event-id absorbs the duplicate.
    let relay_handle = if !cfg.relay_webhook_url.is_empty() {
        let subscriber = RelaySubscriber {
            name: "verification-engine".to_string(),
            webhook_url: cfg.relay_webhook_url.clone(),
            hmac_secret: cfg.relay_hmac_secret.expose_secret().to_string(),
        };
        let relay = OutboxRelay::new(pool.clone(), subscriber)
            .with_poll_interval(std::time::Duration::from_secs(
                cfg.relay_poll_interval_seconds,
            ))
            .with_metrics(metrics.clone());
        let cancel_relay = cancel.clone();
        info!(
            webhook = %cfg.relay_webhook_url,
            poll_interval_s = cfg.relay_poll_interval_seconds,
            "outbox relay (HTTP) enabled"
        );
        Some(tokio::spawn(async move {
            relay.run(cancel_relay).await;
        }))
    } else {
        info!("outbox relay (HTTP) disabled (RELAY_WEBHOOK_URL not set)");
        None
    };

    // R-LOOP-2 — Kafka producer. Enabled when KAFKA_BROKERS is set AND
    // RELAY_TRANSPORT == "kafka". Either condition unset preserves
    // existing HTTP-only behaviour. The producer reads the same outbox
    // table the HTTP relay reads — both compete for the same rows
    // when both are active, with the producer winning whichever row
    // it claims first (UPDATE ... WHERE dispatched_at IS NULL is the
    // serialisation point).
    let kafka_handle = if !cfg.kafka_brokers.is_empty() && cfg.relay_transport == "kafka" {
        match KafkaProducer::build_producer(&cfg.kafka_brokers) {
            Ok(producer_client) => {
                let kafka = KafkaProducer::new(
                    pool.clone(),
                    producer_client,
                    cfg.kafka_declaration_topic.clone(),
                )
                .with_poll_interval(std::time::Duration::from_secs(
                    cfg.relay_poll_interval_seconds,
                ))
                .with_metrics(metrics.clone());
                let cancel_kafka = cancel.clone();
                info!(
                    brokers = %cfg.kafka_brokers,
                    topic = %cfg.kafka_declaration_topic,
                    poll_interval_s = cfg.relay_poll_interval_seconds,
                    "kafka producer enabled (R-LOOP-2)"
                );
                Some(tokio::spawn(async move {
                    let backend: std::sync::Arc<dyn RelayBackend> = std::sync::Arc::new(kafka);
                    backend.run(cancel_kafka).await;
                }))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "kafka producer build failed — continuing with HTTP relay only"
                );
                None
            }
        }
    } else {
        info!(
            kafka_brokers_set = !cfg.kafka_brokers.is_empty(),
            relay_transport = %cfg.relay_transport,
            "kafka producer disabled (R-LOOP-2 inactive)"
        );
        None
    };

    // COMP-2 — outbox retention worker. Always spawned so a single
    // cancellation surface covers every background task; when
    // OUTBOX_RETENTION_DAYS=0 (test/dev default) it logs "disabled"
    // and waits on the cancel token, doing no work. Production
    // operators opt in by setting the env explicitly.
    warn_if_misconfigured(
        cfg.outbox_retention_days,
        cfg.outbox_retention_interval_seconds,
    );
    let retention = OutboxRetention::new(pool.clone())
        .with_retention_days(cfg.outbox_retention_days)
        .with_interval(std::time::Duration::from_secs(
            cfg.outbox_retention_interval_seconds,
        ))
        .with_metrics(metrics.clone());
    info!(
        retention_days = cfg.outbox_retention_days,
        interval_s = cfg.outbox_retention_interval_seconds,
        "outbox retention worker spawning"
    );
    let cancel_retention = cancel.clone();
    let retention_handle = tokio::spawn(async move {
        retention.run(cancel_retention).await;
    });

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

    // Wait for the relay + retention worker + gRPC server to finish.
    if let Some(h) = relay_handle {
        let _ = h.await;
    }
    if let Some(h) = kafka_handle {
        let _ = h.await;
    }
    let _ = retention_handle.await;
    if let Some(h) = grpc_handle {
        let _ = h.await;
    }

    info!("recor-declaration stopped");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => info!("SIGTERM received; shutting down"),
        _ = sigint.recv() => info!("SIGINT received; shutting down"),
    }
}
