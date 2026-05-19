//! Composition root for the RÉCOR Entity service (IDENTITY-1).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use recor_entity_service::api::AppState;
use recor_entity_service::application::{
    DissolveEntityUseCase, GetEntityUseCase, RegisterEntityUseCase, SearchEntitiesUseCase,
    UpdateEntityUseCase,
};
use recor_entity_service::config::Config;
use recor_entity_service::infrastructure::{IdempotencyStore, PostgresEntityRepository};

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env().context("loading configuration from environment")?;
    let _tracing_guard = recor_entity_service::observability::init(&cfg)
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;

    info!(
        service = %cfg.service_name,
        env = %cfg.environment,
        bind = %cfg.bind_addr,
        "recor-entity-service starting"
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

    let repository = Arc::new(PostgresEntityRepository::new(pool.clone()));
    repository
        .run_migrations()
        .await
        .context("running database migrations")?;
    info!("migrations applied");

    let register = Arc::new(RegisterEntityUseCase::new(repository.clone()));
    let get = Arc::new(GetEntityUseCase::new(repository.clone()));
    let search = Arc::new(SearchEntitiesUseCase::new(repository.clone()));
    let update = Arc::new(UpdateEntityUseCase::new(repository.clone()));
    let dissolve = Arc::new(DissolveEntityUseCase::new(repository.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool.clone()));

    let base_url = std::env::var("RECOR_BASE_URL").unwrap_or_else(|_| {
        format!("http://{}", cfg.bind_addr.trim_start_matches("0.0.0.0:"))
    });

    // OIDC verifier (R-AUTH-1 shared crate).
    let oidc = if cfg.oidc_issuer_url.is_empty() {
        info!("OIDC verifier disabled (dev mode — OIDC_ISSUER_URL unset)");
        None
    } else {
        use recor_entity_service::api::oidc::OidcVerifierBuilder;
        let builder = OidcVerifierBuilder::new(
            cfg.oidc_issuer_url.clone(),
            cfg.oidc_audience.clone(),
        )
        .subject_claim(cfg.oidc_subject_claim.clone());
        let v = recor_entity_service::api::OidcVerifier::discover_with_builder(builder)
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

    let metrics = recor_entity_service::metrics::Metrics::new()
        .map_err(|e| anyhow::anyhow!("prometheus registry init failed: {e}"))?;
    info!("prometheus metrics registry initialised");

    let admin_principals: HashSet<String> =
        cfg.admin_principals_list().into_iter().collect();
    if admin_principals.is_empty() {
        info!("admin principals list is empty; POST /v1/entities/{{id}}/dissolve will refuse all callers");
    }

    let metrics_for_separate_listener = metrics.clone();
    let metrics_bind_addr = cfg.metrics_bind_addr.clone();

    let app_state = AppState {
        register_usecase: register,
        get_usecase: get,
        search_usecase: search,
        update_usecase: update,
        dissolve_usecase: dissolve,
        idempotency,
        base_url,
        is_dev: cfg.is_dev(),
        idempotency_ttl_seconds: cfg.idempotency_ttl_seconds,
        oidc,
        metrics,
        admin_principals: Arc::new(admin_principals),
    };

    // FIND-007: when METRICS_BIND_ADDR is set, /metrics moves to a
    // separate listener and the main router omits the route.
    let expose_metrics_on_main = metrics_bind_addr.is_empty();
    let router = recor_entity_service::api::router(app_state, &cfg, expose_metrics_on_main);

    let addr: SocketAddr = cfg.bind_addr.parse().context("parsing bind address")?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;
    info!(%addr, "listening");

    let cancel = CancellationToken::new();

    let metrics_handle = if !metrics_bind_addr.is_empty() {
        let m_addr: SocketAddr = metrics_bind_addr
            .parse()
            .context("parsing metrics_bind_addr")?;
        let m_listener = TcpListener::bind(m_addr)
            .await
            .with_context(|| format!("binding metrics listener {m_addr}"))?;
        let m_router = recor_entity_service::api::metrics_only_router(
            metrics_for_separate_listener,
        );
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

    if let Some(h) = metrics_handle {
        let _ = h.await;
    }

    info!("recor-entity-service stopped");
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
