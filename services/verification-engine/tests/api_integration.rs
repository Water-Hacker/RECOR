//! FIND-014 integration test — V-engine HTTP surface end-to-end.
//!
//! Brings up Postgres via testcontainers, spawns the router on an
//! ephemeral port, and exercises the operator surface (system probes,
//! OpenAPI artefacts, the per-case RBAC gate on `GET /v1/verifications/{id}`,
//! and the admin allowlist on the DLQ endpoints).
//!
//! The pipeline itself is exercised by `pipeline_integration.rs`; this
//! file focuses on the auth/observability surface so a regression in
//! the FIND-002 / FIND-004 / FIND-007 / FIND-013 gates fails CI here
//! before a real deployment sees it.
//!
//! Run with:
//!   cargo test -p recor-verification-engine --test api_integration \
//!     -- --ignored --nocapture
//!
//! Requires Docker (testcontainers spawns Postgres).

use std::net::TcpListener;
use std::sync::Arc;

use reqwest::StatusCode;
use secrecy::SecretString;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;

use recor_verification_engine::api::AppState;
use recor_verification_engine::application::{
    GetVerificationUseCase, PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::application::stages::{
    AdverseMediaStub, CrossSourceStub, IdentityAuthenticationStage, PatternDetectionStub,
    PepStub, SanctionsStub, SchemaValidationStage,
};
use recor_verification_engine::config::Config;
use recor_verification_engine::domain::{LaneThresholds, Stage};
use recor_verification_engine::infrastructure::{
    OutboxAdminStore, PostgresMockBunec, PostgresVerificationRepository,
};

/// Wrapper that keeps the testcontainers Postgres alive for the
/// lifetime of the test. The base URL is the ephemeral
/// `http://127.0.0.1:<port>` the router was spawned on.
struct TestService {
    base_url: String,
    _postgres: ContainerAsync<Postgres>,
}

/// Boot a fresh V-engine against a testcontainers Postgres. The
/// service is configured admin-only (`spiffe://recor.cm/test-admin`
/// is on the allowlist); HMAC inbound is enabled with a known secret.
async fn spawn_service() -> TestService {
    // Postgres 17 matches production (declaration's integration tests
    // use the same image; pg 13+ ships `gen_random_uuid` in core).
    let pg = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres container");
    let port = pg.get_host_port_ipv4(5432).await.expect("pg port");
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("connect pool");

    let repository = Arc::new(PostgresVerificationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations apply");

    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));
    let bunec = Arc::new(PostgresMockBunec::new(pool.clone()));

    // The 7 stages the production wiring uses today. Stubs return
    // vacuous outcomes; the FIND-009 follow-up swaps them for the
    // real implementations.
    let stages: Vec<Arc<dyn Stage>> = vec![
        Arc::new(SchemaValidationStage::new()),
        Arc::new(IdentityAuthenticationStage::new(bunec.clone())),
        Arc::new(SanctionsStub::new()),
        Arc::new(PepStub::new()),
        Arc::new(AdverseMediaStub::new()),
        Arc::new(PatternDetectionStub::new()),
        Arc::new(CrossSourceStub::new()),
    ];
    let orchestrator =
        Arc::new(PipelineOrchestrator::new(stages, LaneThresholds::default()));
    let submit_usecase = Arc::new(SubmitVerificationUseCase::new(
        orchestrator,
        repository.clone(),
    ));
    let get_usecase = Arc::new(GetVerificationUseCase::new(repository.clone()));

    let metrics = recor_verification_engine::metrics::Metrics::new()
        .expect("metrics registry");

    // Pick an ephemeral port the OS isn't using.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    let cfg = test_config(&bind_addr, &database_url);

    let admin_principals: std::collections::HashSet<String> =
        cfg.admin_principals_list().into_iter().collect();
    let app_state = AppState {
        submit_usecase,
        get_usecase,
        repository,
        outbox_admin,
        is_dev: cfg.is_dev(),
        oidc: None,
        metrics,
        admin_principals: Arc::new(admin_principals),
    };

    // Single-port posture for the test: /metrics stays on the main
    // listener so the test can scrape it.
    let router = recor_verification_engine::api::router(app_state, &cfg, true);
    let tcp = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("rebind for axum");
    tokio::spawn(async move {
        axum::serve(tcp, router).await.expect("axum serve");
    });

    // Wait briefly for the server to become ready.
    let client = reqwest::Client::new();
    for _ in 0..40 {
        if let Ok(resp) = client
            .get(format!("http://{bind_addr}/healthz"))
            .send()
            .await
        {
            if resp.status() == StatusCode::OK {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    TestService {
        base_url: format!("http://{bind_addr}"),
        _postgres: pg,
    }
}

fn test_config(bind_addr: &str, database_url: &str) -> Config {
    // Public-field constructor — the typed Config struct exposes
    // every field, so we don't need to round-trip through env vars.
    Config {
        bind_addr: bind_addr.to_string(),
        metrics_bind_addr: String::new(),
        database_url: SecretString::from(database_url.to_string()),
        db_pool_max_connections: 5,
        otlp_endpoint: String::new(),
        log_filter: "warn".to_string(),
        service_name: "recor-verification-engine-test".to_string(),
        environment: "dev".to_string(),
        oidc_issuer_url: String::new(),
        oidc_audience: String::new(),
        oidc_subject_claim: "sub".to_string(),
        http_timeout_seconds: 10,
        inbound_hmac_secret: SecretString::from(
            "test-inbound-hmac-secret-do-not-use-in-prod".to_string(),
        ),
        inbound_hmac_secret_old: SecretString::from(String::new()),
        writeback_url: String::new(),
        writeback_hmac_secret: SecretString::from(String::new()),
        writeback_poll_interval_seconds: 5,
        writeback_max_attempts: 12,
        admin_principals: "spiffe://recor.cm/test-admin".to_string(),
        log_redaction: String::new(),
        outbox_retention_days: 0,
        outbox_retention_interval_seconds: 86_400,
        log_redaction_key: SecretString::from(String::new()),
        kafka_brokers: String::new(),
        kafka_consumer_group: "recor-verification-engine-test".to_string(),
        kafka_declaration_topic: "recor.declaration.events.v1".to_string(),
        verification_transport: "http".to_string(),
        auth_transport: "hmac".to_string(),
        spiffe_socket: String::new(),
        spiffe_id_self: "spiffe://recor.cm/verification".to_string(),
        spiffe_id_peer: "spiffe://recor.cm/declaration".to_string(),
    }
}

// ─── Operational surface ─────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn healthz_returns_ok() {
    let svc = spawn_service().await;
    let resp = reqwest::get(format!("{}/healthz", svc.base_url))
        .await
        .expect("GET /healthz");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("healthz JSON");
    assert_eq!(body["status"].as_str(), Some("ok"));
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn readyz_returns_ok_when_db_reachable() {
    let svc = spawn_service().await;
    let resp = reqwest::get(format!("{}/readyz", svc.base_url))
        .await
        .expect("GET /readyz");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("readyz JSON");
    assert_eq!(body["status"].as_str(), Some("ready"));
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn metrics_exposition_is_prometheus_text() {
    let svc = spawn_service().await;
    let resp = reqwest::get(format!("{}/metrics", svc.base_url))
        .await
        .expect("GET /metrics");
    assert_eq!(resp.status(), StatusCode::OK);
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        ctype.starts_with("text/plain"),
        "metrics content-type should be Prometheus text exposition, got {ctype}"
    );
    let body = resp.text().await.expect("metrics body");
    // Sanity check: at least one HELP line is present (the registry
    // emits one for every registered metric).
    assert!(
        body.contains("# HELP"),
        "metrics body missing # HELP lines; first 500 bytes: {}",
        &body[..body.len().min(500)]
    );
}

// ─── OpenAPI surface (FIND-013 closure) ──────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn openapi_json_is_served() {
    let svc = spawn_service().await;
    let resp = reqwest::get(format!("{}/openapi.json", svc.base_url))
        .await
        .expect("GET /openapi.json");
    assert_eq!(resp.status(), StatusCode::OK);
    let spec: Value = resp.json().await.expect("openapi JSON");
    assert_eq!(spec["openapi"].as_str(), Some("3.1.0"));
    // The four V-engine top-level paths must all be present.
    let paths = spec["paths"]
        .as_object()
        .expect("openapi paths object");
    for expected in [
        "/healthz",
        "/readyz",
        "/v1/verifications",
        "/v1/verifications/{case_id}",
        "/v1/internal/verification-outbox-dlq",
        "/v1/internal/verification-outbox-dlq/{id}/replay",
        "/v1/internal/declaration-events",
    ] {
        assert!(
            paths.contains_key(expected),
            "missing path {expected} in /openapi.json; have: {:?}",
            paths.keys().collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn docs_route_serves_scalar_ui() {
    let svc = spawn_service().await;
    let resp = reqwest::get(format!("{}/docs", svc.base_url))
        .await
        .expect("GET /docs");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.expect("docs HTML");
    // Scalar's HTML always carries the literal "scalar" in the body.
    assert!(
        body.to_lowercase().contains("scalar"),
        "docs HTML does not look like the Scalar UI"
    );
}

// ─── Auth gates (FIND-002 / FIND-004 / FIND-007) ─────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn get_verification_refuses_unauthenticated() {
    let svc = spawn_service().await;
    let case_id = "00000000-0000-0000-0000-000000000000";
    let resp = reqwest::get(format!("{}/v1/verifications/{case_id}", svc.base_url))
        .await
        .expect("GET /v1/verifications/...");
    // No bearer + no dev-principal header ⇒ 401 from the auth middleware.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn get_verification_returns_404_for_unknown_case_to_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let case_id = "00000000-0000-0000-0000-000000000000";
    let resp = client
        .get(format!("{}/v1/verifications/{case_id}", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/test-admin")
        .send()
        .await
        .expect("GET /v1/verifications/... as admin");
    // The case does not exist; admin sees 404.
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn submit_verification_refuses_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/random-declarant")
        .header("content-type", "application/json")
        // The body is intentionally minimal — auth runs before the
        // body parser, so 403 fires here regardless.
        .body("{}")
        .send()
        .await
        .expect("POST /v1/verifications as non-admin");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn dlq_list_refuses_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/internal/verification-outbox-dlq",
            svc.base_url
        ))
        .header(
            "X-Recor-Dev-Principal",
            "spiffe://recor.cm/random-declarant",
        )
        .send()
        .await
        .expect("GET DLQ as non-admin");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn dlq_list_returns_empty_for_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/internal/verification-outbox-dlq",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/test-admin")
        .send()
        .await
        .expect("GET DLQ as admin");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("DLQ list JSON");
    assert_eq!(body["total"].as_i64(), Some(0));
    assert!(
        body["items"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "expected empty items array on a fresh service"
    );
}

// ─── Internal webhook HMAC gate (FIND-002 + dual-secret) ─────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn internal_webhook_refuses_unsigned_request() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/internal/declaration-events", svc.base_url))
        .header("content-type", "application/json")
        .body(r#"{"event_id":"00000000-0000-0000-0000-000000000000","event_type":"x","event_version":1,"aggregate_id":"00000000-0000-0000-0000-000000000000","payload":{}}"#)
        .send()
        .await
        .expect("POST internal webhook unsigned");
    // No X-RECOR-Signature header ⇒ 401 from the HMAC gate.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
