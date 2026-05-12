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
use testcontainers_modules::testcontainers::ImageExt;
use time::OffsetDateTime;
use uuid::Uuid;

use recor_declaration::api::AppState;
use recor_declaration::application::{
    AmendDeclarationUseCase, CorrectDeclarationUseCase, GetDeclarationUseCase,
    ListByPrincipalUseCase, RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase,
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
    // Match production (`postgres:17-alpine`). The default
    // testcontainers Postgres is `11-alpine` where `gen_random_uuid`
    // is not in core; pg 13+ ships it directly so we pin to 17.
    let postgres_container = Postgres::default()
        .with_tag("17-alpine")
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
    let amend = Arc::new(AmendDeclarationUseCase::new(repository.clone()));
    let correct = Arc::new(CorrectDeclarationUseCase::new(repository.clone()));
    let list_by_principal =
        Arc::new(ListByPrincipalUseCase::new(repository.clone()));
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
        amend_usecase: amend,
        correct_usecase: correct,
        list_by_principal_usecase: list_by_principal,
        idempotency,
        outbox_admin,
        base_url: format!("http://{bind_addr}"),
        is_dev: true,
        idempotency_ttl_seconds: 3600,
        oidc: None,
        metrics: recor_declaration::metrics::Metrics::new().expect("metrics registry"),
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
        rate_limit_per_min: 0,
        rate_limit_burst: 0,
        log_redaction: String::new(),
        log_redaction_key: SecretString::from(String::new()),
        // R-DECL-8: empty disables the gRPC server. The REST-only
        // integration tests do not exercise the gRPC surface.
        grpc_bind_addr: String::new(),
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

    // Canonical bytes must match the server's struct field order
    // EXACTLY (entity_id, declarant_principal, declarant_role, kind,
    // effective_from, beneficial_owners, nonce_hex). `serde_json::Value`
    // (from `json!{}`) sorts keys alphabetically; serialising a struct
    // preserves declaration order. Assemble by hand to stay byte-parity
    // with `api::rest::canonical_payload_bytes`.
    let canonical_string = format!(
        "{{\"entity_id\":\"{entity_id}\",\
\"declarant_principal\":\"{principal}\",\
\"declarant_role\":\"self\",\
\"kind\":\"incorporation\",\
\"effective_from\":\"2026-01-01\",\
\"beneficial_owners\":[{{\"person_id\":\"{person_id}\",\"ownership_basis_points\":10000,\"interest_kind\":\"equity\"}}],\
\"nonce_hex\":\"{nonce_hex}\"}}"
    );
    let canonical_bytes = canonical_string.into_bytes();
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

// COMP-1 — Data-subject access (GDPR right of access + portability).
//
// The end-to-end leakage refusal property: a declarant who submits as
// principal A and then queries `/v1/declarations/by-principal` as A
// sees only A's records, and a different declarant B sees only B's.
// This is the property the entire compliance posture depends on —
// breaking it would let any authenticated declarant enumerate the
// platform's full register.

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn by_principal_endpoint_returns_only_callers_declarations() {
    let svc = spawn_service().await;
    let client = reqwest::Client::new();

    // Two distinct declarants with their own keys.
    let key_a = SigningKey::from_bytes(&[42u8; 32]);
    let key_b = SigningKey::from_bytes(&[99u8; 32]);
    let principal_a = "spiffe://recor.cm/comp-1-declarant-alpha";
    let principal_b = "spiffe://recor.cm/comp-1-declarant-beta";

    // Submit two declarations under A.
    for _ in 0..2 {
        let body = build_request_body(
            principal_a,
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &key_a,
        );
        let resp = client
            .post(format!("{}/v1/declarations", svc.base_url))
            .header("x-recor-dev-principal", principal_a)
            .json(&body)
            .send()
            .await
            .expect("post A");
        assert_eq!(resp.status(), StatusCode::CREATED, "A submit must succeed");
    }

    // Submit one declaration under B.
    let body_b = build_request_body(
        principal_b,
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        &key_b,
    );
    let resp = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal_b)
        .json(&body_b)
        .send()
        .await
        .expect("post B");
    assert_eq!(resp.status(), StatusCode::CREATED, "B submit must succeed");

    // A queries by-principal — must see exactly 2 rows, both A's.
    let resp = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", principal_a)
        .send()
        .await
        .expect("get A by-principal");
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: Value = resp.json().await.expect("A body");
    assert_eq!(payload["principal"], json!(principal_a));
    assert_eq!(
        payload["count"], json!(2),
        "principal A must see exactly two of their own rows; full body: {payload}"
    );
    let declarations = payload["declarations"]
        .as_array()
        .expect("declarations array on A response");
    assert_eq!(declarations.len(), 2);
    for row in declarations {
        assert_eq!(
            row["declarant_principal"], json!(principal_a),
            "every returned row must belong to the querying principal; leakage detected"
        );
        // D15: the receipt hash MUST be present so the declarant can
        // re-verify each receipt offline.
        let receipt = row["receipt_hash_hex"].as_str().unwrap_or_default();
        assert_eq!(
            receipt.len(),
            64,
            "receipt_hash_hex must be a 64-char hex string for offline re-verification"
        );
    }

    // B queries by-principal — must see exactly 1 row.
    let resp = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", principal_b)
        .send()
        .await
        .expect("get B by-principal");
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: Value = resp.json().await.expect("B body");
    assert_eq!(payload["principal"], json!(principal_b));
    assert_eq!(payload["count"], json!(1));
    let declarations = payload["declarations"]
        .as_array()
        .expect("declarations array on B response");
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0]["declarant_principal"], json!(principal_b));
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn by_principal_endpoint_refuses_unauthenticated() {
    // D14 fail-closed: the endpoint must refuse a request that arrives
    // with no authentication. Otherwise an attacker could enumerate
    // every declarant by guessing principals via the query string —
    // but there IS no query string here, so the refusal must come from
    // the auth gate.
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .send()
        .await
        .expect("get without auth");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn by_principal_endpoint_returns_empty_for_first_time_caller() {
    // A declarant who has never submitted has the right to know that
    // no data is held — empty list, 200 OK, not 404.
    let svc = spawn_service().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/declarations/by-principal", svc.base_url))
        .header("x-recor-dev-principal", "spiffe://recor.cm/never-submitted")
        .send()
        .await
        .expect("get never-submitted");
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: Value = resp.json().await.expect("empty body");
    assert_eq!(payload["count"], json!(0));
    assert_eq!(
        payload["declarations"].as_array().map(|a| a.len()),
        Some(0),
        "declarations array must be empty, not absent"
    );
}

// Silence unused-import warnings during incremental compile.
#[allow(dead_code)]
fn _force(_t: OffsetDateTime) {}
