//! Integration tests for per-principal rate limiting (OPS-1).
//!
//! Brings up Postgres via testcontainers + a service instance with
//! rate limiting ENABLED (`RATE_LIMIT_PER_MIN`/`RATE_LIMIT_BURST` set).
//! The "regular" `api_integration.rs` keeps rate limiting disabled
//! (per_min=0) so it doesn't interact with throughput-sensitive cases
//! (idempotency replay, duplicate-submit).
//!
//! Tests covered (gated `#[ignore]` so they only run with Docker
//! available — `cargo test --test rate_limit_integration -- --ignored`):
//!
//!   * `burst_plus_one_returns_429_with_retry_after` —
//!     same principal hammers POST /v1/declarations with burst+1
//!     signed requests; the (burst+1)th gets 429 with Retry-After.
//!   * `two_principals_do_not_interfere` —
//!     two principals each submit at the burst rate; neither's
//!     bucket affects the other.
//!   * `get_endpoints_are_not_rate_limited` —
//!     the GET projection endpoint accepts 30 rapid pulls (portal
//!     polls verification status every ~3s).

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

/// Spawn a service instance with rate limiting configured.
///
/// `per_min = 0` would disable the limiter (see Config docs); we pass
/// concrete numbers and verify the 429 boundary in the tests below.
async fn spawn_service_with_rate_limit(per_min: u32, burst: u32) -> TestService {
    // Match production (`postgres:17-alpine` in
    // docker-compose.integration.yaml). The testcontainers-modules
    // crate defaults to `postgres:11-alpine`, where `gen_random_uuid`
    // requires `pgcrypto` extension — pg 13+ has it in core and the
    // migrations rely on the core function.
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

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    let cfg = test_config(&bind_addr, &database_url, per_min, burst);

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

    // Wait briefly for the server to become reachable.
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

fn test_config(
    bind_addr: &str,
    database_url: &str,
    rate_limit_per_min: u32,
    rate_limit_burst: u32,
) -> Config {
    use secrecy::SecretString;
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
        cors_allowed_origins: String::new(),
        rate_limit_per_min,
        rate_limit_burst,
        log_redaction: String::new(),
        log_redaction_key: SecretString::from(String::new()),
        // R-DECL-8: empty disables the gRPC server in this REST-only
        // test harness.
        grpc_bind_addr: String::new(),
        // COMP-2: retention disabled in tests.
        outbox_retention_days: 0,
        outbox_retention_interval_seconds: 86_400,
    }
}

fn build_request_body(
    principal: &str,
    declaration_id: Uuid,
    entity_id: Uuid,
    person_id: Uuid,
    key: &SigningKey,
) -> Value {
    let nonce_hex = hex::encode(uuid::Uuid::new_v4().as_bytes());

    // Field order MUST match the server's `canonical_payload_bytes`
    // struct order (entity_id, declarant_principal, declarant_role,
    // kind, effective_from, beneficial_owners, nonce_hex). `json!{}`
    // sorts keys alphabetically; we assemble the bytes by hand to
    // preserve byte-parity with the server (D15 cryptographic
    // provenance: any divergence breaks the Ed25519 signature
    // verification).
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
    let signature = key.sign(&canonical_bytes);
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(key.verifying_key().to_bytes());

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

/// (Burst=10, per_min=60) → first 10 same-principal POSTs go through;
/// the 11th gets 429 with Retry-After.
///
/// Acceptance criterion #1 from the OPS-1 brief.
#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn burst_plus_one_returns_429_with_retry_after() {
    // Tight quota so a single test pass doesn't accidentally exceed
    // the bucket on a slow CI runner: 60/min sustained, burst 10.
    let svc = spawn_service_with_rate_limit(60, 10).await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[7u8; 32]);
    let principal = "spiffe://recor.cm/rate-limit-test-1";

    // Fire 10 valid submissions. Each carries a unique
    // declaration_id/entity_id/person_id/nonce so the server can't
    // short-circuit any of them on idempotency or duplicate-submit
    // grounds; the only thing that should stop a request is the
    // rate limiter.
    for i in 0..10 {
        let body = build_request_body(
            principal,
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &key,
        );
        let resp = client
            .post(format!("{}/v1/declarations", svc.base_url))
            .header("x-recor-dev-principal", principal)
            .json(&body)
            .send()
            .await
            .expect("post");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "request {} (within burst) should succeed",
            i + 1
        );
    }

    // 11th request — should be blocked.
    let body = build_request_body(
        principal,
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        &key,
    );
    let resp = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .expect("post");
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "11th submission MUST be rate-limited"
    );

    let retry_after = resp
        .headers()
        .get("retry-after")
        .expect("Retry-After header MUST be present on 429")
        .to_str()
        .expect("Retry-After parseable")
        .parse::<u64>()
        .expect("Retry-After is an integer");
    assert!(
        (1..=60).contains(&retry_after),
        "Retry-After should be 1..=60s, got {retry_after}"
    );

    let body: Value = resp.json().await.expect("json body");
    assert_eq!(body["error"]["kind"], json!("rate_limited"));
    assert!(
        body["error"]["retry_after_seconds"].is_number(),
        "rate_limited body MUST carry retry_after_seconds"
    );
}

/// Two principals each push the bucket to the limit, in parallel.
/// Neither should affect the other.
///
/// Acceptance criterion #2 from the OPS-1 brief.
#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn two_principals_do_not_interfere() {
    let svc = spawn_service_with_rate_limit(60, 5).await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[19u8; 32]);
    let alice = "spiffe://recor.cm/rate-limit-alice";
    let bob = "spiffe://recor.cm/rate-limit-bob";

    // Alice consumes her full burst (5 successful submissions).
    for i in 0..5 {
        let body = build_request_body(
            alice,
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &key,
        );
        let resp = client
            .post(format!("{}/v1/declarations", svc.base_url))
            .header("x-recor-dev-principal", alice)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "alice request {} should pass",
            i + 1
        );
    }

    // Alice's next request hits the bucket floor.
    let body = build_request_body(
        alice,
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        &key,
    );
    let r = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", alice)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "alice's 6th request MUST be throttled"
    );

    // Bob has not consumed any of his burst. He should still get all
    // five through cleanly — his bucket is independent of alice's.
    for i in 0..5 {
        let body = build_request_body(
            bob,
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &key,
        );
        let resp = client
            .post(format!("{}/v1/declarations", svc.base_url))
            .header("x-recor-dev-principal", bob)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "bob request {} MUST not be affected by alice's throttling",
            i + 1
        );
    }
}

/// GET endpoints (used by the portal to poll verification status
/// every ~3 seconds) must NOT be rate-limited. The brief explicitly
/// excludes them.
///
/// Acceptance criterion #4: portal polls verification status every 3s
/// — would self-DoS otherwise.
#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn get_endpoints_are_not_rate_limited() {
    // Tight burst on POST (2) makes any accidental sharing with GET
    // surface immediately. GET is a separate route — should not be
    // affected.
    let svc = spawn_service_with_rate_limit(60, 2).await;
    let client = reqwest::Client::new();
    let key = SigningKey::from_bytes(&[33u8; 32]);
    let principal = "spiffe://recor.cm/rate-limit-getter";

    // Submit one declaration so we have something to GET.
    let declaration_id = Uuid::now_v7();
    let body = build_request_body(
        principal,
        declaration_id,
        Uuid::now_v7(),
        Uuid::now_v7(),
        &key,
    );
    let resp = client
        .post(format!("{}/v1/declarations", svc.base_url))
        .header("x-recor-dev-principal", principal)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Now poll that declaration 30 times rapidly. With burst=2 on
    // POSTs, if GET were on the same bucket, the third poll would
    // 429. Real expectation: all 30 succeed.
    for i in 0..30 {
        let r = client
            .get(format!("{}/v1/declarations/{}", svc.base_url, declaration_id))
            .header("x-recor-dev-principal", principal)
            .send()
            .await
            .expect("get");
        assert_eq!(
            r.status(),
            StatusCode::OK,
            "GET poll {} MUST not be rate-limited",
            i + 1
        );
    }
}
