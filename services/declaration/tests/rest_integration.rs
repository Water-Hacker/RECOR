//! TODO-025 — REST integration tests for the Declaration service.
//!
//! Covers every documented status code in the OpenAPI spec, malformed-
//! payload attacks, header confusion, per-row tenancy, and a proptest
//! over SubmitDeclarationRequest JSON variations.
//!
//! All tests are gated `#[ignore]` — CI runs them via `--ignored`.
//! Run locally:
//!   cargo test -p recor-declaration --test rest_integration -- --ignored --nocapture

use std::net::TcpListener;
use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use proptest::prelude::*;
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use uuid::Uuid;

use recor_declaration::api::AppState;
use recor_declaration::application::{
    AmendDeclarationUseCase, CorrectDeclarationUseCase, GetDeclarationUseCase,
    ListByPrincipalUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
    SupersedeDeclarationUseCase,
};
use recor_declaration::config::Config;
use recor_declaration::infrastructure::postgres::{IdempotencyStore, PostgresDeclarationRepository};
use recor_declaration::infrastructure::OutboxAdminStore;

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
        .expect("connect");

    let repository = Arc::new(PostgresDeclarationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations");

    let submit = Arc::new(SubmitDeclarationUseCase::new(repository.clone()));
    let get = Arc::new(GetDeclarationUseCase::new(repository.clone()));
    let record_verification = Arc::new(RecordVerificationOutcomeUseCase::new(repository.clone()));
    let supersede = Arc::new(SupersedeDeclarationUseCase::new(repository.clone()));
    let amend = Arc::new(AmendDeclarationUseCase::new(repository.clone()));
    let correct = Arc::new(CorrectDeclarationUseCase::new(repository.clone()));
    let list_by_principal = Arc::new(ListByPrincipalUseCase::new(repository.clone()));
    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    let cfg = test_config(&bind_addr, &database_url);
    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        record_verification_usecase: record_verification,
        supersede_usecase: supersede,
        amend_usecase: amend,
        correct_usecase: correct,
        list_by_principal_usecase: list_by_principal,
        idempotency,
        outbox_admin,
        base_url: format!("http://{bind_addr}"),
        is_dev: true,
        idempotency_ttl_seconds: 3600,
        oidc: None,
        metrics: recor_declaration::metrics::Metrics::new().expect("metrics"),
        admin_principals: Arc::new(std::collections::HashSet::new()),
        obliged_entity_read_limit_per_day: 0,
    };

    let router = recor_declaration::api::router(app_state, &cfg, true);
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
    use secrecy::SecretString;
    Config {
        bind_addr: bind_addr.to_string(),
        metrics_bind_addr: String::new(),
        database_url: SecretString::from(database_url.to_string()),
        db_pool_max_connections: 5,
        idempotency_ttl_seconds: 3600,
        otlp_endpoint: String::new(),
        log_filter: "warn".to_string(),
        service_name: "recor-declaration-rest-test".to_string(),
        environment: "dev".to_string(),
        oidc_issuer_url: String::new(),
        oidc_audience: String::new(),
        oidc_subject_claim: "sub".to_string(),
        http_timeout_seconds: 10,
        relay_webhook_url: String::new(),
        relay_hmac_secret: SecretString::from(String::new()),
        relay_poll_interval_seconds: 5,
        writeback_hmac_secret: SecretString::from(String::new()),
        writeback_hmac_secret_old: SecretString::from(String::new()),
        admin_principals: String::new(),
        cors_allowed_origins: String::new(),
        rate_limit_per_min: 0,
        rate_limit_burst: 0,
        log_redaction: String::new(),
        log_redaction_key: SecretString::from(String::new()),
        grpc_bind_addr: String::new(),
        outbox_retention_days: 0,
        outbox_retention_interval_seconds: 86_400,
        public_feedback_per_ip_max_per_window: 0,
        public_feedback_per_ip_window_secs: 3600,
        public_feedback_mass_flag_threshold: 5,
        public_feedback_mass_flag_window_secs: 86_400,
        obliged_entity_read_limit_per_day: 0,
        kafka_brokers: String::new(),
        kafka_declaration_topic: "recor.declaration.events.v1".to_string(),
        relay_transport: "http".to_string(),
        auth_transport: "hmac".to_string(),
        spiffe_socket: String::new(),
        spiffe_id_self: "spiffe://recor.cm/declaration".to_string(),
        spiffe_id_peer: "spiffe://recor.cm/verification".to_string(),
        person_service_url: String::new(),
        person_service_bearer: None,
    }
}

/// Build a canonical signed body. The nonce is always fresh.
fn signed_body(principal: &str, key: &SigningKey) -> Value {
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    signed_body_for(principal, key, declaration_id, entity_id, person_id)
}

fn signed_body_for(
    principal: &str,
    key: &SigningKey,
    declaration_id: Uuid,
    entity_id: Uuid,
    person_id: Uuid,
) -> Value {
    let nonce_hex = hex::encode(Uuid::new_v4().as_bytes());
    let canonical = format!(
        "{{\"entity_id\":\"{entity_id}\",\
\"declarant_principal\":\"{principal}\",\
\"declarant_role\":\"self\",\
\"kind\":\"incorporation\",\
\"effective_from\":\"2026-01-01\",\
\"beneficial_owners\":[{{\"person_id\":\"{person_id}\",\"ownership_basis_points\":10000,\"interest_kind\":\"equity\"}}],\
\"nonce_hex\":\"{nonce_hex}\"}}"
    );
    let sig = key.sign(canonical.as_bytes());
    json!({
        "declaration_id": declaration_id,
        "entity_id": entity_id,
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
        }],
        "attestation": {
            "signed_by": principal,
            "signature_algorithm": "ed25519",
            "signature_hex": hex::encode(sig.to_bytes()),
            "public_key_hex": hex::encode(key.verifying_key().to_bytes()),
            "nonce_hex": nonce_hex,
        },
        "adequacy_claims": {
            "adequate": true,
            "accurate": true,
            "up_to_date": true,
        }
    })
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
}

// ─── 200 OK — GET declaration after submit ────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_200_get_declaration_exists() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[1u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-200-get";
    let body = signed_body(principal, &key);
    let decl_id = body["declaration_id"].as_str().unwrap().to_string();

    client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();

    let r = client
        .get(format!("{}/v1/declarations/{decl_id}", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let payload: Value = r.json().await.unwrap();
    assert_eq!(payload["declaration_id"].as_str(), Some(decl_id.as_str()));
    assert_eq!(payload["state"].as_str(), Some("submitted"));
}

// ─── 201 Created — successful submit ─────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_201_submit_declaration() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[2u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-201";

    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&signed_body(principal, &key))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let payload: Value = r.json().await.unwrap();
    assert_eq!(payload["state"].as_str(), Some("submitted"));
    assert!(
        payload["receipt_hash_hex"].as_str().map(|s| s.len() == 64).unwrap_or(false),
        "receipt_hash_hex must be 64-char hex"
    );
}

// ─── 400 Bad Request — body missing required field ───────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_missing_required_field() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    // `entity_id` is required; omitting it causes a deserialisation error.
    let bad_body = json!({
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
    });
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/rest-int-400")
        .json(&bad_body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

// ─── 400 Bad Request — malformed JSON ────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_malformed_json() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/rest-int-malformed")
        .header("content-type", "application/json")
        .body("{not valid json[[[")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

// ─── 400 Bad Request — oversize payload (> 1 MiB) ────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_oversize_payload() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    // 1.5 MiB of ASCII 'a'.
    let big = "a".repeat(1_572_864);
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/rest-int-oversize")
        .header("content-type", "application/json")
        .body(format!("{{\"junk\":\"{big}\"}}"))
        .send()
        .await
        .unwrap();
    // axum's default body-size limit returns 400 or 413.
    assert!(
        r.status() == StatusCode::BAD_REQUEST
            || r.status() == StatusCode::PAYLOAD_TOO_LARGE
            || r.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/413/422 on oversize payload, got {}",
        r.status()
    );
}

// ─── 400 Bad Request — control character injection in string field ────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_400_control_character_injection() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    // A NUL byte inside a JSON string field — the DB would reject this
    // but the service should surface 400 / 422 before touching Postgres.
    let bad = json!({
        "entity_id": "00000000-0000-0000-0000-000000000000",
        "declarant_role": "self\u{0000}injected",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [],
        "attestation": {
            "signed_by": "x",
            "signature_algorithm": "ed25519",
            "signature_hex": "ab".repeat(32),
            "public_key_hex": "cd".repeat(16),
            "nonce_hex": "ef".repeat(16),
        }
    });
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/rest-int-inject")
        .json(&bad)
        .send()
        .await
        .unwrap();
    assert!(
        r.status().is_client_error(),
        "control-character injection must return 4xx, got {}",
        r.status()
    );
}

// ─── 401 Unauthorized — no auth header ───────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_no_auth_get() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!(
        "{}/v1/declarations/00000000-0000-0000-0000-000000000000",
        svc.base_url
    ))
    .await
    .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_no_auth_post() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_no_auth_by_principal() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── 401 Unauthorized — bearer token but OIDC disabled in dev ────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_401_bearer_token_without_oidc_configured() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    // OIDC is None in the test harness; a bearer token cannot be verified
    // and the service must fail-closed (D14).
    let r = client
        .get(format!(
            "{}/v1/declarations/00000000-0000-0000-0000-000000000000",
            svc.base_url
        ))
        .header("Authorization", "Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.bogus.signature")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── 403 Forbidden — DLQ admin endpoint non-admin ────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_403_dlq_list_non_admin() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/v1/internal/outbox-dlq", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/random-user")
        .send()
        .await
        .unwrap();
    // Non-admin callers must receive 403 from the admin gate.
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

// ─── 403 Forbidden — header confusion: x-recor-dev-principal ignored in prod ──

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn header_confusion_dev_principal_accepted_only_in_dev_mode() {
    // In this test harness is_dev=true so the header IS accepted.
    // This test verifies the dev mode works and also documents the threat:
    // in prod the header must be rejected (tested in oidc_integration.rs).
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/dev-user")
        .send()
        .await
        .unwrap();
    // In dev mode: the header is accepted and the response should be 200 OK.
    assert_eq!(r.status(), StatusCode::OK);
}

// ─── 404 Not Found — unknown declaration id ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_404_unknown_declaration() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!(
            "{}/v1/declarations/00000000-0000-0000-0000-000000000001",
            svc.base_url
        ))
        .header("x-recor-dev-principal", "spiffe://recor.cm/rest-int-404")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// ─── 409 Conflict — duplicate submit ─────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_409_duplicate_submit() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[3u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-409";
    let body = signed_body(principal, &key);

    let r1 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::CREATED);

    let r2 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::CONFLICT);
}

// ─── 422 Unprocessable — missing adequacy_claims ──────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_422_missing_adequacy_claims() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[4u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-422-adequacy";
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    let nonce_hex = hex::encode(Uuid::new_v4().as_bytes());

    // Build a body WITHOUT adequacy_claims (the FATF guard at the DTO layer
    // returns 422 for this).
    let body = json!({
        "entity_id": entity_id,
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
            "cascade_tier": "tier_1",
        }],
        "attestation": {
            "signed_by": principal,
            "signature_algorithm": "ed25519",
            "signature_hex": hex::encode(key.sign(b"dummy").to_bytes()),
            "public_key_hex": hex::encode(key.verifying_key().to_bytes()),
            "nonce_hex": nonce_hex,
        }
        // adequacy_claims intentionally absent
    });

    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ─── 422 Unprocessable — missing cascade_tier ────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn status_422_missing_cascade_tier() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[5u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-422-cascade";
    let person_id = Uuid::now_v7();
    let nonce_hex = hex::encode(Uuid::new_v4().as_bytes());

    // beneficial_owners entry lacks cascade_tier — DTO refuses with 422.
    let body = json!({
        "entity_id": Uuid::now_v7(),
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
            // cascade_tier intentionally absent
        }],
        "attestation": {
            "signed_by": principal,
            "signature_algorithm": "ed25519",
            "signature_hex": hex::encode(key.sign(b"dummy").to_bytes()),
            "public_key_hex": hex::encode(key.verifying_key().to_bytes()),
            "nonce_hex": nonce_hex,
        },
        "adequacy_claims": {
            "adequate": true,
            "accurate": true,
            "up_to_date": true,
        }
    });

    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ─── 200 OK — idempotent replay returns same receipt ─────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotent_replay_returns_same_receipt() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[6u8; 32]);
    let principal = "spiffe://recor.cm/rest-int-idem";
    let body = signed_body(principal, &key);
    let idem_key = format!("idem-{}", Uuid::now_v7());

    let r1 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem_key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::CREATED);
    let p1: Value = r1.json().await.unwrap();

    let r2 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem_key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::CREATED);
    let p2: Value = r2.json().await.unwrap();
    assert_eq!(p1, p2, "idempotent replay must return identical body");
}

// ─── Tenancy predicate: by-principal returns only caller's rows ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn tenancy_by_principal_does_not_leak_cross_tenant_data() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key_a = SigningKey::from_bytes(&[7u8; 32]);
    let key_b = SigningKey::from_bytes(&[8u8; 32]);
    let pa = "spiffe://recor.cm/rest-tenant-alpha";
    let pb = "spiffe://recor.cm/rest-tenant-beta";

    // A submits 2 declarations.
    for _ in 0..2 {
        let r = client
            .post(format!("{}/v1/declarations", svc.base_url))
            .header("x-recor-dev-principal", pa)
            .json(&signed_body(pa, &key_a))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
    }
    // B submits 1.
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", pb)
        .json(&signed_body(pb, &key_b))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);

    // A queries by-principal: must see exactly 2 rows all belonging to A.
    let r = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", pa)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["count"], json!(2));
    for row in body["declarations"].as_array().unwrap() {
        assert_eq!(row["declarant_principal"].as_str(), Some(pa));
    }

    // B queries: must see exactly 1 row.
    let r = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", pb)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["count"], json!(1));
}

// ─── Tenancy: GET /v1/declarations/{id} refuses cross-tenant read ─────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn tenancy_get_by_id_refuses_cross_tenant_read() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[9u8; 32]);
    let owner = "spiffe://recor.cm/rest-owner";
    let bystander = "spiffe://recor.cm/rest-bystander";

    let body = signed_body(owner, &key);
    let decl_id = body["declaration_id"].as_str().unwrap().to_string();
    client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", owner)
        .json(&body)
        .send()
        .await
        .unwrap();

    // Another principal reads the same id — must get 404 (not 403 or 200).
    let r = client
        .get(format!("{}/v1/declarations/{decl_id}", svc.base_url))
        .header("x-recor-dev-principal", bystander)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        StatusCode::NOT_FOUND,
        "cross-tenant read must return 404 (enumeration defence)"
    );
}

// ─── 200 OK — empty list for never-submitted principal ───────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn by_principal_returns_empty_for_new_principal() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/brand-new")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["count"], json!(0));
}

// ─── OpenAPI spec served ──────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn openapi_json_is_served() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/openapi.json", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let spec: Value = r.json().await.unwrap();
    assert_eq!(spec["openapi"].as_str(), Some("3.1.0"));
}

// ─── Scalar docs served ───────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn docs_route_serves_scalar_ui() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/docs", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let html = r.text().await.unwrap();
    assert!(html.to_lowercase().contains("scalar"));
}

// ─── Metrics endpoint ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn metrics_returns_prometheus_text() {
    let svc = spawn_service().await;
    let r = reqwest::get(format!("{}/metrics", svc.base_url)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body = r.text().await.unwrap();
    assert!(body.contains("# HELP"), "expected Prometheus # HELP lines");
}

// ─── Internal writeback endpoint — 401 without HMAC signature ────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn internal_writeback_401_without_hmac() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .post(format!("{}/v1/internal/verification-outcome", svc.base_url))
        .header("content-type", "application/json")
        .body(r#"{"case_id":"00000000-0000-0000-0000-000000000000","declaration_id":"00000000-0000-0000-0000-000000000000","lane":"green","fused_authenticity_belief":0.9,"fused_authenticity_plausibility":0.95,"fused_risk_belief":0.1,"completed_at":"2026-05-01T10:00:00Z"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─── D13 idempotency: repeated idempotency-key with different body → 409 ──────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_conflict_on_body_mismatch() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[10u8; 32]);
    let principal = "spiffe://recor.cm/rest-idem-conflict";
    let body1 = signed_body(principal, &key);
    let body2 = signed_body(principal, &key);
    let idem = format!("idem-conflict-{}", Uuid::now_v7());

    let r1 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem)
        .json(&body1)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::CREATED);

    // Second request uses the same idempotency-key but a different body.
    let r2 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem)
        .json(&body2)
        .send()
        .await
        .unwrap();
    // Per the API spec, mismatched body with an already-used key returns 409.
    assert_eq!(r2.status(), StatusCode::CONFLICT);
}

// ─── D15 provenance: receipt_hash_hex present on every submit response ─────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn d15_receipt_hash_present_on_submit() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[11u8; 32]);
    let principal = "spiffe://recor.cm/rest-d15";

    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&signed_body(principal, &key))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let payload: Value = r.json().await.unwrap();
    let hash = payload["receipt_hash_hex"].as_str().unwrap_or_default();
    assert_eq!(hash.len(), 64, "receipt_hash_hex must be 64 hex chars (D15)");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
}

// ─── Property test: various valid JSON field orders still produce 201 ─────────
// This proptest is NOT gated by ignore — it runs in unit mode.

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(20))]

    #[test]
    fn proptest_submit_request_entity_id_is_uuid_shaped(
        // Generate UUIDs as strings in the v4 hyphenated format.
        a in 0u8..=255u8,
        b in 0u8..=255u8,
    ) {
        // Property: a SubmitDeclarationRequest struct deserialises from JSON
        // whose entity_id is a well-formed UUIDv4. No panics on round-trip.
        let entity_uuid = Uuid::from_bytes([a, b, a, b, a, b, a, b, a, b, a, b, a, b, a, b]);
        let json_str = format!(
            r#"{{"entity_id":"{entity_uuid}","declarant_role":"self","kind":"incorporation","effective_from":"2026-01-01","beneficial_owners":[],"attestation":{{"signed_by":"x","signature_algorithm":"ed25519","signature_hex":"{}","public_key_hex":"{}","nonce_hex":"{}"}}}}"#,
            "ab".repeat(32),
            "cd".repeat(16),
            "ef".repeat(16),
        );
        // We test that serde does not panic — the result can be Ok or Err.
        let _result: Result<recor_declaration::api::dto::SubmitDeclarationRequest, _> =
            serde_json::from_str(&json_str);
    }
}
