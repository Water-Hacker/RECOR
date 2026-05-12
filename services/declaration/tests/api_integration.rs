//! End-to-end integration test for the Declaration service.
//!
//! Brings up Postgres via testcontainers, spawns the service against it,
//! runs the full happy-path + idempotency + duplicate-submit + auth
//! scenarios via reqwest, and tears down.
//!
//! Run with: cargo test --test api_integration -- --nocapture
//!
//! Requires Docker daemon reachable (testcontainers provisions Postgres
//! in a container).

use std::net::TcpListener;
use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use time::OffsetDateTime;
use uuid::Uuid;

use recor_declaration::api::AppState;
use recor_declaration::application::{
    GetDeclarationUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
    SupersedeDeclarationUseCase,
};
use recor_declaration::config::Config;
use recor_declaration::infrastructure::postgres::{
    IdempotencyStore, PostgresDeclarationRepository,
};
use recor_declaration::infrastructure::OutboxAdminStore;

struct TestService {
    base_url: String,
    _postgres: ContainerAsync<Postgres>,
}

async fn spawn_service() -> TestService {
    let postgres_container = Postgres::default()
        .start()
        .await
        .expect("start postgres container");
    let port = postgres_container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("connect to postgres");

    let repository = Arc::new(PostgresDeclarationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations");

    let submit = Arc::new(SubmitDeclarationUseCase::new(repository.clone()));
    let get = Arc::new(GetDeclarationUseCase::new(repository.clone()));
    let record_verification =
        Arc::new(RecordVerificationOutcomeUseCase::new(repository.clone()));
    let supersede = Arc::new(SupersedeDeclarationUseCase::new(repository.clone()));
    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool));

    // Bind to an ephemeral port.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    // Construct config from a fixed string set rather than environment.
    let cfg = test_config(&bind_addr, &database_url);

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        record_verification_usecase: record_verification,
        supersede_usecase: supersede,
        idempotency,
        outbox_admin,
        base_url: format!("http://{bind_addr}"),
        is_dev: true,
        idempotency_ttl_seconds: 3600,
        oidc: None,
    };
    let router = recor_declaration::api::router(app_state, &cfg);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("rebind for axum");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum serve");
    });

    // Wait briefly for the server to be ready.
    let client = reqwest::Client::new();
    for _ in 0..40 {
        if let Ok(resp) = client
            .get(&format!("http://{bind_addr}/healthz"))
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
        _postgres: postgres_container,
    }
}

fn test_config(bind_addr: &str, database_url: &str) -> Config {
    use secrecy::SecretString;
    // We can't easily construct Config without env; build it manually with
    // serde from a struct-literal alternative. Since `Config` is a public
    // struct with all-public fields, just construct it directly.
    Config {
        bind_addr: bind_addr.to_string(),
        database_url: SecretString::from(database_url.to_string()),
        db_pool_max_connections: 5,
        idempotency_ttl_seconds: 3600,
        otlp_endpoint: String::new(),
        log_filter: "warn".to_string(),
        service_name: "recor-declaration-test".to_string(),
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
    }
}

fn sign_payload(key: &SigningKey, payload: &[u8]) -> (String, String) {
    let signature = key.sign(payload);
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(key.verifying_key().to_bytes());
    (signature_hex, public_key_hex)
}

fn build_request_body(
    principal: &str,
    declaration_id: Uuid,
    entity_id: Uuid,
    person_id: Uuid,
    key: &SigningKey,
) -> Value {
    let nonce_hex = hex::encode(uuid::Uuid::new_v4().as_bytes());

    // Canonical bytes match what the server canonicalises in
    // api::rest::canonical_payload_bytes.
    let canonical = json!({
        "entity_id": entity_id,
        "declarant_principal": principal,
        "declarant_role": "self",
        "kind": "incorporation",
        "effective_from": "2026-01-01",
        "beneficial_owners": [{
            "person_id": person_id,
            "ownership_basis_points": 10000,
            "interest_kind": "equity",
        }],
        "nonce_hex": &nonce_hex,
    });
    let canonical_bytes = serde_json::to_vec(&canonical).unwrap();
    let (signature_hex, public_key_hex) = sign_payload(key, &canonical_bytes);

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
            "signature_hex": signature_hex,
            "public_key_hex": public_key_hex,
            "nonce_hex": nonce_hex,
        }
    })
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn happy_path_submit_and_get() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[42u8; 32]);
    let principal = "spiffe://recor.cm/integration-declarant";
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();

    let body =
        build_request_body(principal, declaration_id, entity_id, person_id, &key);

    let resp = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), StatusCode::CREATED, "submit should be 201");
    let payload: Value = resp.json().await.expect("submit body");
    assert_eq!(payload["declaration_id"], json!(declaration_id));
    assert_eq!(payload["state"], json!("submitted"));
    assert!(payload["receipt_hash_hex"].as_str().unwrap().len() == 64);

    let resp = client
        .get(format!("{}/v1/declarations/{declaration_id}", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status(), StatusCode::OK);
    let projection: Value = resp.json().await.expect("get body");
    assert_eq!(projection["declaration_id"], json!(declaration_id));
    assert_eq!(projection["declarant_principal"], json!(principal));
    assert_eq!(projection["aggregate_version"], json!(1));
    assert_eq!(projection["state"], json!("submitted"));
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn duplicate_submit_returns_409() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[42u8; 32]);
    let principal = "spiffe://recor.cm/integration-declarant";
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    let body = build_request_body(principal, declaration_id, entity_id, person_id, &key);

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

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotent_replay_returns_same_receipt() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[42u8; 32]);
    let principal = "spiffe://recor.cm/integration-declarant";
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    let body = build_request_body(principal, declaration_id, entity_id, person_id, &key);
    let idem = format!("idem-{}", Uuid::now_v7());

    let r1 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::CREATED);
    let p1: Value = r1.json().await.unwrap();

    let r2 = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .header("idempotency-key", &idem)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::CREATED);
    let p2: Value = r2.json().await.unwrap();
    assert_eq!(p1, p2, "idempotent replay should return identical body");
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn unauthenticated_get_returns_401() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/declarations/{}", svc.base_url, Uuid::now_v7()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn healthz_is_public() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let r = client
        .get(format!("{}/healthz", svc.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

// Silence unused-import warnings during incremental compile.
#[allow(dead_code)]
fn _force(_t: OffsetDateTime) {}
