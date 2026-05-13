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
        AdverseMediaStub, CrossSourceStub, IdentityAuthenticationStage, PatternDetectionStub,
        PepStub, SanctionsStub, SchemaValidationStage,
    },
    GetVerificationUseCase, PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::config::Config;
use recor_verification_engine::domain::{LaneThresholds, Stage};
use recor_verification_engine::infrastructure::{
    KafkaConsumer, OutboxAdminStore, PostgresMockBunec, PostgresVerificationRepository,
    VerificationOutboxRelay, VerificationOutboxRetention, WritebackSubscriber,
};
use recor_verification_engine::infrastructure::retention::warn_if_misconfigured;

#[tokio::main]
async fn main() -> Result<()> {
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

    let bunec = Arc::new(PostgresMockBunec::new(pool.clone()));

    let stages: Vec<Arc<dyn Stage>> = vec![
        Arc::new(SchemaValidationStage::new()),
        Arc::new(IdentityAuthenticationStage::new(bunec.clone())),
        Arc::new(SanctionsStub::new()),
        Arc::new(PepStub::new()),
        Arc::new(AdverseMediaStub::new()),
        Arc::new(PatternDetectionStub::new()),
        Arc::new(CrossSourceStub::new()),
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

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        repository,
        outbox_admin,
        is_dev: cfg.is_dev(),
        oidc,
        metrics: metrics.clone(),
    };

    let router = recor_verification_engine::api::router(app_state, &cfg);
    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing bind addr")?;
    let listener = TcpListener::bind(addr).await.context("binding")?;
    info!(%addr, "listening");

    // Cancellation token shared with the writeback relay so shutdown
    // is coordinated with the HTTP server.
    let cancel = CancellationToken::new();

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
