//! `audit-verifier` binary entrypoint.

use std::sync::Arc;

use anyhow::Context as _;
use audit_verifier::{
    auth::AuthConfig,
    config::VerifierConfig,
    fabric_client::HttpFabricClient,
    handlers::{router, AppState},
    projection::{DeclarationApiProjection, PostgresProjectionRepo, ProjectionRepo},
};
use recor_auth_oidc::{OidcVerifier, OidcVerifierBuilder};
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
    // TODO-041: prefer the Declaration service HTTP API over the
    // direct DB read when `DECLARATION_API_URL` is configured. Empty
    // value (the dev posture) falls back to the Postgres projection
    // repo so local development without the Declaration service still
    // works. Production deployments MUST set DECLARATION_API_URL —
    // the direct Postgres read is a cross-service contract leak
    // (D17 zero-trust violation) and the fallback is dev-only.
    let declaration_api_url = std::env::var("DECLARATION_API_URL").unwrap_or_default();
    let declaration_api_bearer = std::env::var("DECLARATION_API_TOKEN")
        .or_else(|_| std::env::var("DECLARATION_API_BEARER_TOKEN"))
        .unwrap_or_default();
    let projection: Arc<dyn ProjectionRepo> = if !declaration_api_url.is_empty() {
        info!(
            declaration_api = %declaration_api_url,
            "audit-verifier using DeclarationApiProjection (HTTP)"
        );
        Arc::new(DeclarationApiProjection::new(
            declaration_api_url,
            declaration_api_bearer,
        ))
    } else {
        tracing::warn!(
            "DECLARATION_API_URL not set; falling back to direct Postgres projection read (dev-only posture)"
        );
        Arc::new(PostgresProjectionRepo::new(pool))
    };

    // FIND-001 (audit Sprint 0): construct the OIDC verifier so the
    // verify endpoint can authenticate every caller. Outside dev, an
    // empty issuer is refused at config load — the Option is None only
    // in dev, where the `X-Recor-Dev-Principal` header is accepted in
    // its place (and the FIND-003 mutual exclusion ensures both paths
    // cannot be active simultaneously).
    let oidc: Option<Arc<OidcVerifier>> = if cfg.oidc_issuer_url.is_empty() {
        None
    } else {
        let builder = OidcVerifierBuilder::new(&cfg.oidc_issuer_url, &cfg.oidc_audience)
            .subject_claim(&cfg.oidc_subject_claim);
        Some(
            OidcVerifier::discover_with_builder(builder)
                .await
                .context("OIDC verifier discovery failed")?,
        )
    };
    let auth = AuthConfig {
        is_dev: cfg.is_dev(),
        oidc,
    };

    let listener = TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("bind {}", cfg.bind_addr))?;

    let app_state = AppState {
        fabric,
        projection,
    };
    axum::serve(listener, router(app_state, auth))
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
