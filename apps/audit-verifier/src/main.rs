//! `audit-verifier` binary entrypoint.

use std::sync::Arc;

use anyhow::Context as _;
use audit_verifier::{
    config::VerifierConfig,
    fabric_client::HttpFabricClient,
    handlers::{router, AppState},
    projection::PostgresProjectionRepo,
};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio::signal;
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
    let cfg = VerifierConfig::from_env().context("load config")?;

    info!(bind = %cfg.bind_addr, gateway = %cfg.gateway_url, "audit-verifier starting");

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(cfg.database_url.expose_secret())
        .await
        .context("connect to postgres")?;

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
    let projection = Arc::new(PostgresProjectionRepo::new(pool));

    let listener = TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("bind {}", cfg.bind_addr))?;

    let app_state = AppState {
        fabric,
        projection,
    };
    axum::serve(listener, router(app_state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("audit-verifier stopped");
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
