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
    RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase, SupersedeDeclarationUseCase,
};
use recor_declaration::config::Config;
use recor_declaration::infrastructure::postgres::{
    IdempotencyStore, PostgresDeclarationRepository,
};
use recor_declaration::infrastructure::{OutboxAdminStore, OutboxRelay, RelaySubscriber};

#[tokio::main]
async fn main() -> Result<()> {
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

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        record_verification_usecase: record_verification,
        supersede_usecase: supersede,
        amend_usecase: amend,
        correct_usecase: correct,
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
            "outbox relay enabled"
        );
        Some(tokio::spawn(async move {
            relay.run(cancel_relay).await;
        }))
    } else {
        info!("outbox relay disabled (RELAY_WEBHOOK_URL not set)");
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

    // Wait for the relay + gRPC server to finish.
    if let Some(h) = relay_handle {
        let _ = h.await;
    }
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
