//! Composition root for the RÉCOR Person service.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use recor_person_service::api::{AppState, OidcVerifier};
use recor_person_service::application::{
    GetPersonUseCase, MergePersonsUseCase, RegisterPersonUseCase, SearchPersonsUseCase,
};
use recor_person_service::config::Config;
use recor_person_service::infrastructure::outbox_admin::OutboxAdminStore;
use recor_person_service::infrastructure::postgres::{
    IdempotencyStore, PostgresPersonRepository,
};
use recor_person_service::infrastructure::relay::{OutboxRelay, RelaySubscriber};
use recor_person_service::infrastructure::retention::{
    warn_if_misconfigured, OutboxRetention,
};

#[tokio::main]
async fn main() -> Result<()> {
    // FIND-018 (audit Sprint 3) / OPS-4: load secrets from Vault
    // before Config::from_env(). Mirror of declaration's wiring —
    // VAULT_ADDR empty ⇒ pure env mode with a startup warn!;
    // VAULT_ADDR set + Vault unreachable ⇒ hard-fail (D14).
    let vault_paths: &[(&str, &[(&str, &str)])] = &[
        (
            "recor/person/database",
            &[("DATABASE_URL", "DATABASE_URL")],
        ),
        (
            "recor/person/oidc",
            &[
                ("OIDC_ISSUER_URL", "OIDC_ISSUER_URL"),
                ("OIDC_AUDIENCE", "OIDC_AUDIENCE"),
            ],
        ),
        (
            "recor/person/observability",
            &[("LOG_REDACTION_KEY", "LOG_REDACTION_KEY")],
        ),
    ];
    recor_vault_client::populate_from_vault(vault_paths)
        .await
        .map_err(|e| anyhow::anyhow!("Vault secret loading failed (D14 fail-closed): {e}"))?;

    let cfg = Config::from_env().context("loading configuration from environment")?;
    let _tracing_guard = recor_person_service::observability::init(&cfg)
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;

    info!(
        service = %cfg.service_name,
        env = %cfg.environment,
        bind = %cfg.bind_addr,
        "recor-person-service starting"
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

    let repository = Arc::new(PostgresPersonRepository::new(pool.clone()));
    repository
        .run_migrations()
        .await
        .context("running database migrations")?;
    info!("migrations applied");

    let register = Arc::new(RegisterPersonUseCase::new(repository.clone()));
    let get = Arc::new(GetPersonUseCase::new(repository.clone()));
    let search = Arc::new(SearchPersonsUseCase::new(repository.clone()));
    let merge = Arc::new(MergePersonsUseCase::new(repository.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool.clone()));
    // TODO-040 — DLQ admin store; shared with the DLQ admin API state.
    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));

    let base_url = std::env::var("RECOR_BASE_URL").unwrap_or_else(|_| {
        format!("http://{}", cfg.bind_addr.trim_start_matches("0.0.0.0:"))
    });

    let oidc = if cfg.oidc_issuer_url.is_empty() {
        info!("OIDC verifier disabled (dev mode — OIDC_ISSUER_URL unset)");
        None
    } else {
        use recor_person_service::api::oidc::OidcVerifierBuilder;
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
            "OIDC verifier ready (JWKS pre-warmed)"
        );
        Some(v)
    };

    let metrics = recor_person_service::metrics::Metrics::new()
        .map_err(|e| anyhow::anyhow!("prometheus registry init failed: {e}"))?;

    // FIND-018 / R-LOOP-3: SPIFFE bootstrap. Hard-fail when the
    // operator chose `AUTH_TRANSPORT=mtls` AND the Workload API
    // cannot hand us an SVID — D14 fail-closed mirrors declaration /
    // V-engine. When `AUTH_TRANSPORT=hmac` (the default), the block
    // is a single warn-free log line.
    let spiffe_metrics = std::sync::Arc::new(
        recor_spiffe::SpiffeMetrics::register(&metrics.registry)
            .map_err(|e| anyhow::anyhow!("spiffe metrics register failed: {e}"))?,
    );
    let spiffe_client = if cfg.mtls_enabled() {
        info!(
            socket = %cfg.spiffe_socket,
            self_id = %cfg.spiffe_id_self,
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
        // TODO(R-LOOP-3-followup): when person-service grows an
        // inbound internal endpoint, wire the peer-SPIFFE-ID gate
        // using `recor_spiffe::enforce_peer_id`. The integration
        // test pattern is in
        // `services/verification-engine/tests/peer_spiffe_id_gate.rs`
        // (FIND-017 closure).
        Some(client)
    } else {
        info!(
            transport = %cfg.auth_transport,
            "AUTH_TRANSPORT=hmac — SPIFFE not bootstrapped"
        );
        None
    };
    let _spiffe = spiffe_client;

    let admin_principals: HashSet<String> =
        cfg.admin_principals_list().into_iter().collect();
    if admin_principals.is_empty() {
        info!("admin endpoints disabled (ADMIN_PRINCIPALS empty)");
    } else {
        info!(count = admin_principals.len(), "admin allowlist loaded");
    }

    // FIND-007: clone the metrics Arc BEFORE moving it into AppState so
    // we can hand a copy to the separate metrics listener below.
    let metrics_for_separate_listener = metrics.clone();
    // COMP-2: clone for the outbox retention worker (TODO-016).
    let state_metrics_for_retention = metrics.clone();
    let metrics_bind_addr = cfg.metrics_bind_addr.clone();

    let app_state = AppState {
        register_usecase: register,
        get_usecase: get,
        search_usecase: search,
        merge_usecase: merge,
        idempotency,
        outbox_admin,
        base_url,
        is_dev: cfg.is_dev(),
        idempotency_ttl_seconds: cfg.idempotency_ttl_seconds,
        oidc,
        metrics,
        admin_principals: Arc::new(admin_principals),
    };

    // FIND-007: separate `/metrics` listener when METRICS_BIND_ADDR set.
    let expose_metrics_on_main = metrics_bind_addr.is_empty();
    let router =
        recor_person_service::api::router(app_state, &cfg, expose_metrics_on_main);

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
        let m_router =
            recor_person_service::api::metrics_only_router(metrics_for_separate_listener);
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

    // TODO-040 — outbox relay worker. Optional: enabled iff
    // OUTBOX_RELAY_TARGET_URL is set. D14 fail-closed: the cross-field
    // check at config-load time guarantees the HMAC secret is present
    // when the URL is.
    let relay_handle = if !cfg.outbox_relay_target_url.is_empty() {
        let subscriber = RelaySubscriber {
            name: cfg.outbox_relay_subscriber_name.clone(),
            webhook_url: cfg.outbox_relay_target_url.clone(),
            hmac_secret: cfg.outbox_relay_hmac_secret.expose_secret().to_string(),
        };
        let relay = OutboxRelay::new(pool.clone(), subscriber)
            .with_poll_interval(std::time::Duration::from_secs(
                cfg.outbox_relay_poll_interval_seconds,
            ))
            .with_batch_size(cfg.outbox_relay_batch_size)
            .with_max_attempts(cfg.outbox_relay_max_dispatch_attempts)
            .with_metrics(state_metrics_for_retention.clone());
        let cancel_relay = cancel.clone();
        info!(
            webhook = %cfg.outbox_relay_target_url,
            subscriber = %cfg.outbox_relay_subscriber_name,
            poll_interval_s = cfg.outbox_relay_poll_interval_seconds,
            batch_size = cfg.outbox_relay_batch_size,
            max_attempts = cfg.outbox_relay_max_dispatch_attempts,
            "person outbox relay enabled"
        );
        Some(tokio::spawn(async move {
            relay.run(cancel_relay).await;
        }))
    } else {
        info!("person outbox relay disabled (OUTBOX_RELAY_TARGET_URL not set)");
        None
    };

    // COMP-2 — outbox retention worker. Always spawned so a single
    // cancellation surface covers every background task; when
    // OUTBOX_RETENTION_DAYS=0 (test/dev default) it logs "disabled" and
    // waits on the cancel token, doing no work. Production operators
    // opt in by setting the env explicitly.
    warn_if_misconfigured(
        cfg.outbox_retention_days,
        cfg.outbox_retention_interval_seconds,
    );
    let retention = OutboxRetention::new(pool.clone())
        .with_retention_days(cfg.outbox_retention_days)
        .with_interval(std::time::Duration::from_secs(
            cfg.outbox_retention_interval_seconds,
        ))
        .with_metrics(state_metrics_for_retention.clone());
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
    let _ = retention_handle.await;
    if let Some(h) = relay_handle {
        if let Err(e) = h.await {
            tracing::warn!(error = ?e, worker = "outbox-relay", "worker join failed during shutdown");
        }
    }
    if let Some(h) = metrics_handle {
        let _ = h.await;
    }
    info!("recor-person-service stopped");
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
