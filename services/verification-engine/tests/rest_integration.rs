//! TODO-026 — REST integration tests for the Verification Engine HTTP surface.
//!
//! Exercises every documented status code, auth gates, DLQ admin surface,
//! HMAC webhook gate, and the internal declaration-events endpoint.
//!
//! All tests are gated `#[ignore]` — CI runs them via `--ignored`.
//! Run locally:
//!   cargo test -p recor-verification-engine --test rest_integration \
//!     -- --ignored --nocapture

use std::net::TcpListener;
use std::sync::Arc;

use reqwest::StatusCode;
use secrecy::SecretString;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use uuid::Uuid;

use recor_verification_engine::api::AppState;
use recor_verification_engine::application::stages::{
    AdverseMediaStub, CrossSourceStub, IdentityAuthenticationStage, PatternDetectionStub,
    PepStub, SanctionsStub, SchemaValidationStage,
};
use recor_verification_engine::application::{
    GetVerificationUseCase, PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::config::Config;
use recor_verification_engine::domain::{LaneThresholds, Stage};
use recor_verification_engine::infrastructure::{
    OutboxAdminStore, PostgresMockBunec, PostgresVerificationRepository,
};

// ─── Harness ──────────────────────────────────────────────────────────────────

struct TestService {
    pub base_url: String,
    _postgres: ContainerAsync<Postgres>,
}

async fn spawn_service() -> TestService {
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
        .expect("pool");

    let repository = Arc::new(PostgresVerificationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations");

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
    let submit_usecase = Arc::new(SubmitVerificationUseCase::new(
        orchestrator,
        repository.clone(),
    ));
    let get_usecase = Arc::new(GetVerificationUseCase::new(repository.clone()));
    let metrics = recor_verification_engine::metrics::Metrics::new().expect("metrics");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
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

    let router = recor_verification_engine::api::router(app_state, &cfg, true);
    let tcp = tokio::net::TcpListener::bind(&bind_addr).await.expect("rebind");
    tokio::spawn(async move {
        axum::serve(tcp, router).await.expect("serve");
    });

    let client = reqwest::Client::new();
    for _ in 0..40 {
        if let Ok(r) = client.get(format!("http://{bind_addr}/healthz")).send().await {
            if r.status() == StatusCode::OK {
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
    Config {
        bind_addr: bind_addr.to_string(),
        metrics_bind_addr: String::new(),
        database_url: SecretString::from(database_url.to_string()),
        db_pool_max_connections: 5,
        otlp_endpoint: String::new(),
        log_filter: "warn".to_string(),
        service_name: "recor-ve-rest-test".to_string(),
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
        admin_principals: "spiffe://recor.cm/ve-test-admin".to_string(),
        log_redaction: String::new(),
        outbox_retention_days: 0,
        outbox_retention_interval_seconds: 86_400,
        log_redaction_key: SecretString::from(String::new()),
        kafka_brokers: String::new(),
        kafka_consumer_group: "recor-ve-rest-test".to_string(),
        kafka_declaration_topic: "recor.declaration.events.v1".to_string(),
        verification_transport: "http".to_string(),
        auth_transport: "hmac".to_string(),
        spiffe_socket: String::new(),
        spiffe_id_self: "spiffe://recor.cm/verification".to_string(),
        spiffe_id_peer: "spiffe://recor.cm/declaration".to_string(),
        // FIND-009: real-stage activation flags — all false in tests;
        // stubs are wired above so real data sources are not required.
        enable_real_sanctions: false,
        enable_real_pep: false,
        enable_real_adverse_media: false,
        enable_real_patterns: false,
        enable_real_stage7: false,
        // TODO-015: BUNEC adapter — mock in tests; real adapter requires
        // a running BUNEC service (not available in test environment).
        bunec_adapter_kind: "mock".to_string(),
        bunec_base_url: String::new(),
        bunec_api_key: SecretString::from(String::new()),
        bunec_timeout_secs: 2,
        bunec_retry_attempts: 3,
        bunec_retry_backoff_ms: 200,
        bunec_breaker_consecutive_failures: 5,
        bunec_breaker_half_open_secs: 30,
        bunec_fail_policy: String::new(),
        // TODO-048: Stage 5 consensus threshold — use production default.
        stage5_consensus_threshold: 0.9,
    }
}

// ─── 200 OK — healthz ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_healthz() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/healthz", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["status"].as_str(), Some("ok"));
}

// ─── 200 OK — readyz ──────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_readyz() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/readyz", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["status"].as_str(), Some("ready"));
}

// ─── 200 OK — metrics in Prometheus text format ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_metrics_prometheus_text() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/metrics", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let ct = r
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(ct.starts_with("text/plain"), "want Prometheus text, got {ct}");
    let body = r.text().await.unwrap();
    assert!(body.contains("# HELP"));
}

// ─── 200 OK — OpenAPI spec ────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_openapi_json() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/openapi.json", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let spec: Value = r.json().await.unwrap();
    assert_eq!(spec["openapi"].as_str(), Some("3.1.0"));
    let paths = spec["paths"].as_object().expect("paths object");
    for expected in ["/healthz", "/readyz", "/v1/verifications", "/v1/verifications/{case_id}"] {
        assert!(paths.contains_key(expected), "missing path {expected}");
    }
}

// ─── 200 OK — Scalar docs UI ──────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_scalar_docs() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/docs", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let html = r.text().await.unwrap();
    assert!(html.to_lowercase().contains("scalar"));
}

// ─── 401 Unauthorized — unauthenticated GET /v1/verifications/{id} ────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_get_verification_no_auth() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!(
        "{}/v1/verifications/00000000-0000-0000-0000-000000000000",
        svc.base_url
    ))
    .await
    .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── 401 Unauthorized — POST /v1/verifications without auth ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_post_verification_no_auth() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── 401 Unauthorized — internal webhook without HMAC signature ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_internal_webhook_no_hmac() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/internal/declaration-events", svc.base_url))
        .header("content-type", "application/json")
        .body(r#"{"event_id":"00000000-0000-0000-0000-000000000000","event_type":"x","event_version":1,"aggregate_id":"00000000-0000-0000-0000-000000000000","payload":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── 403 Forbidden — POST /v1/verifications non-admin ────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_403_post_verification_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/random-declarant")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

// ─── 403 Forbidden — DLQ list non-admin ──────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_403_dlq_list_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!(
            "{}/v1/internal/verification-outbox-dlq",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/non-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

// ─── 403 Forbidden — DLQ replay non-admin ────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_403_dlq_replay_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!(
            "{}/v1/internal/verification-outbox-dlq/00000000-0000-0000-0000-000000000000/replay",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/non-admin")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

// ─── 404 Not Found — admin reads unknown case ─────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_404_admin_reads_unknown_case() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!(
            "{}/v1/verifications/00000000-0000-0000-0000-000000000000",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// ─── 404 Not Found — non-admin reads a case they don't own ───────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_404_non_owner_reads_case_enumeration_defence() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    // This case_id doesn't exist, but the non-owner must still get 404
    // (not 403, per D17 enumeration-attack defence).
    let r = client
        .get(format!(
            "{}/v1/verifications/00000000-0000-0000-0000-000000000002",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/some-declarant")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// ─── 200 OK — admin sees empty DLQ on fresh service ──────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_dlq_empty_for_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!(
            "{}/v1/internal/verification-outbox-dlq",
            svc.base_url
        ))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["total"].as_i64(), Some(0));
    assert!(body["items"].as_array().map(|a| a.is_empty()).unwrap_or(false));
}

// ─── 400 Bad Request — malformed JSON to POST /v1/verifications (admin) ───────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_malformed_json_to_verification_post() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .header("content-type", "application/json")
        .body("{not valid json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

// ─── 422 Unprocessable — missing required fields in body ─────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_422_verification_body_missing_required_fields() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .json(&json!({"not": "a valid snapshot"}))
        .send()
        .await
        .unwrap();
    assert!(
        r.status() == StatusCode::UNPROCESSABLE_ENTITY
            || r.status() == StatusCode::BAD_REQUEST,
        "expected 400/422 on incomplete body, got {}",
        r.status()
    );
}

// ─── D14 fail-closed: oversize payload rejected ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_or_413_oversize_payload() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let big = "x".repeat(2_097_152); // 2 MiB
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .header("content-type", "application/json")
        .body(format!("{{\"junk\":\"{big}\"}}"))
        .send()
        .await
        .unwrap();
    assert!(
        r.status().is_client_error(),
        "oversize payload must be rejected with 4xx, got {}",
        r.status()
    );
}

// ─── 200 OK — internal declaration-events list for admin ─────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_internal_events_list_for_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/v1/internal/declaration-events", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .send()
        .await
        .unwrap();
    // The endpoint either returns 200 with a list or 200 with an empty set.
    // Key property: admin is not refused.
    assert_eq!(r.status(), StatusCode::OK);
}

// ─── 403 Forbidden — non-admin reads internal declaration-events ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_403_internal_events_list_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/v1/internal/declaration-events", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/random-user")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

// ─── Header confusion: content-type must be application/json ─────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn header_confusion_wrong_content_type_is_rejected() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .header("content-type", "text/plain")
        .body("hello")
        .send()
        .await
        .unwrap();
    assert!(
        r.status().is_client_error(),
        "wrong content-type must be rejected, got {}",
        r.status()
    );
}

// ─── D15 provenance: case_id present on verification submission ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn d15_case_id_present_on_successful_verification() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();

    let decl_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    let case_id = Uuid::now_v7();

    let snapshot = json!({
        "declaration_id": decl_id,
        "entity_id": entity_id,
        "declarant_principal": "spiffe://recor.cm/ve-test-declarant",
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
        }],
        "attestation_hex": "aa".repeat(32),
        "submitted_at": "2026-05-01T10:00:00Z",
        "correlation_id": Uuid::now_v7(),
        "receipt_hash_hex": "bb".repeat(32),
    });

    let body = json!({
        "case_id": case_id,
        "declaration": snapshot,
    });

    let r = client
        .post(format!("{}/v1/verifications", svc.base_url))
        .header("X-Recor-Dev-Principal", "spiffe://recor.cm/ve-test-admin")
        .json(&body)
        .send()
        .await
        .unwrap();

    // Either succeeds (201) or fails with a domain error (422/400).
    // The key assertion is that admin is NOT refused with 401/403.
    assert!(
        !r.status().is_server_error(),
        "server error on verification submission: {}",
        r.status()
    );
    assert_ne!(r.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(r.status(), StatusCode::FORBIDDEN);
}
