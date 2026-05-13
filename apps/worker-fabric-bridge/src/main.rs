//! `worker-fabric-bridge` binary entrypoint.

use std::sync::Arc;

use anyhow::Context as _;
use fabric_bridge::{BridgeConfig, FabricBridge};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{info, warn};

use worker_fabric_bridge::{
    config::WorkerConfig,
    dlq::PostgresDlqRepo,
    handlers::{router, AppState},
    metrics::{BridgeMetricsAdapter, WorkerMetrics},
    processor::EventProcessor,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    // Load .env in dev — production passes config via the platform.
    let _ = dotenvy::dotenv();

    let cfg = WorkerConfig::from_env().context("loading worker config from env")?;

    if cfg.transport != "http" {
        warn!(
            transport = %cfg.transport,
            "non-http transport requested; only http is wired today. The Kafka consumer is deferred to R-LOOP-2."
        );
    }

    info!(bind = %cfg.bind_addr, gateway = %cfg.gateway_url, "worker-fabric-bridge starting");

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(cfg.database_url.expose_secret())
        .await
        .context("connect to postgres")?;

    // Skeleton migration: try a one-shot SQL execute. In production the
    // operator runs migrations out-of-band — this is here so the dev
    // surface bootstraps cleanly.
    if let Err(e) = pool
        .execute(include_str!("../migrations/0001_create_fabric_bridge_dlq.sql"))
        .await
    {
        warn!(error = ?e, "DLQ migration apply failed (may already be applied)");
    }

    let metrics = Arc::new(WorkerMetrics::new());
    let bridge_metrics = Arc::new(BridgeMetricsAdapter::new(metrics.clone()));

    let bridge_cfg = BridgeConfig {
        gateway_url: cfg.gateway_url.clone(),
        channel: cfg.channel.clone(),
        chaincode: cfg.chaincode.clone(),
        max_attempts: cfg.max_attempts,
        backoff_base: cfg.backoff_base,
        request_timeout: cfg.request_timeout,
        bearer_token: cfg
            .gateway_bearer_token
            .as_ref()
            .map(|t| t.expose_secret().to_string()),
    };
    let bridge = Arc::new(
        FabricBridge::new(bridge_cfg)
            .context("build fabric bridge")?
            .with_metrics(bridge_metrics),
    );

    let dlq = Arc::new(PostgresDlqRepo::new(pool.clone()));
    let processor = Arc::new(
        EventProcessor::new(bridge, dlq).with_dlq_metric(metrics.dlq_writes_total.clone()),
    );

    let app_state = AppState {
        processor,
        metrics,
        hmac_secret: cfg.hmac_secret,
    };

    let listener = TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("bind {}", cfg.bind_addr))?;
    info!(addr = %cfg.bind_addr, "listening");

    axum::serve(listener, router(app_state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("worker-fabric-bridge stopped");
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
