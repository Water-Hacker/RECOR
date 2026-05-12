//! Composition root for the RÉCOR Verification Engine.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing::{error, info};

use recor_verification_engine::api::AppState;
use recor_verification_engine::application::{
    stages::{
        AdverseMediaStub, CrossSourceStub, IdentityAuthenticationStage, PatternDetectionStub,
        PepStub, SanctionsStub, SchemaValidationStage,
    },
    GetVerificationUseCase, PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::config::Config;
use recor_verification_engine::domain::{LaneThresholds, Stage};
use recor_verification_engine::infrastructure::{PostgresMockBunec, PostgresVerificationRepository};

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
    let get = Arc::new(GetVerificationUseCase::new(repository.clone()));

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        repository,
        is_dev: cfg.is_dev(),
    };

    let router = recor_verification_engine::api::router(app_state, &cfg);
    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing bind addr")?;
    let listener = TcpListener::bind(addr).await.context("binding")?;
    info!(%addr, "listening");

    let serve = axum::serve(listener, router).with_graceful_shutdown(shutdown_signal());
    if let Err(e) = serve.await {
        error!(error = ?e, "server error");
        return Err(anyhow::anyhow!(e));
    }
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
