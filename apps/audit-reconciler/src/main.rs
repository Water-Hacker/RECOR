//! `audit-reconciler` binary entrypoint. Wires the config + Postgres
//! pool + Fabric client + reconciliation loop + operational HTTP
//! surface. Graceful shutdown on SIGTERM/SIGINT.

use std::sync::Arc;

use anyhow::Context as _;
use audit_reconciler::{
    handlers::{router, AppState},
    PostgresEventLogRepo, ReconcilerConfig, ReconcilerLoop, ReconcilerMetrics,
};
use audit_verifier::fabric_client::HttpFabricClient;
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    let _ = dotenvy::dotenv();
    let cfg = ReconcilerConfig::from_env().context("load config")?;
    info!(
        bind = %cfg.bind_addr,
        gateway = %cfg.gateway_url,
        interval_s = cfg.reconcile_interval.as_secs(),
        grace_s = cfg.grace_period.as_secs(),
        lookback_s = cfg.lookback.as_secs(),
        "audit-reconciler starting"
    );

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(cfg.database_url.expose_secret())
        .await
        .context("connect to postgres")?;

    let metrics = Arc::new(
        ReconcilerMetrics::new()
            .map_err(|e| anyhow::anyhow!("prometheus registry init failed: {e}"))?,
    );

    let repo = Arc::new(PostgresEventLogRepo::new(pool.clone()));

    let fabric = Arc::new(
        HttpFabricClient::new(
            &cfg.gateway_url,
            &cfg.channel,
            &cfg.chaincode,
            cfg.request_timeout,
            cfg.gateway_bearer_token
                .as_ref()
                .map(|s| s.expose_secret().to_string()),
        )
        .context("init fabric client")?,
    );

    let cancel = CancellationToken::new();
    let reconciler = Arc::new(ReconcilerLoop::new(
        repo,
        fabric,
        metrics.clone(),
        cfg.reconcile_interval,
        cfg.grace_period,
        cfg.lookback,
        cfg.max_declarations_per_run,
    ));
    let reconciler_cancel = cancel.clone();
    let reconciler_handle = tokio::spawn(async move {
        reconciler.run(reconciler_cancel).await;
    });

    // Operational HTTP surface. Bound on the main listener — the
    // FIND-007 separate-port pattern is not applied here because the
    // reconciler has no business port to protect; /metrics is the
    // only surface that matters and is already in-cluster only.
    let app_state = AppState {
        metrics: metrics.clone(),
        pool,
    };
    let listener = TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("bind {}", cfg.bind_addr))?;
    let serve_cancel = cancel.clone();
    axum::serve(listener, router(app_state))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            serve_cancel.cancel();
        })
        .await?;

    let _ = reconciler_handle.await;
    info!("audit-reconciler stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            s.recv().await;
        } else {
            std::future::pending::<()>().await
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
