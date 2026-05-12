//! End-to-end integration test for the Declaration service's gRPC
//! surface (R-DECL-8).
//!
//! Brings up Postgres via testcontainers, spawns both the REST server
//! AND the gRPC server against the same `AppState`, submits a
//! declaration via the generated tonic client, then queries the REST
//! GET endpoint to assert the same data round-trips. This is the
//! acceptance criterion in the R-DECL-8 brief.
//!
//! Run with: `cargo test --test grpc_integration -- --ignored --nocapture`
//!
//! Requires a Docker daemon reachable (testcontainers spins up Postgres).
//!
//! The test is `#[ignore]`-gated like the other integration tests so
//! a plain `cargo test` does not need docker.

use std::net::TcpListener;
use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use reqwest::StatusCode;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use uuid::Uuid;

use recor_declaration::api::{AppState, DeclarationGrpcService, GrpcAuthConfig};
use recor_declaration::application::{
    AmendDeclarationUseCase, CorrectDeclarationUseCase, GetDeclarationUseCase,
    RecordVerificationOutcomeUseCase, SubmitDeclarationUseCase, SupersedeDeclarationUseCase,
};
use recor_declaration::config::Config;
use recor_declaration::infrastructure::postgres::{
    IdempotencyStore, PostgresDeclarationRepository,
};
use recor_declaration::infrastructure::OutboxAdminStore;

// Pull in the generated client/types from the in-tree `grpc::proto`
// module so we don't have to re-include the proto in the tests crate.
use recor_declaration::api::grpc::proto;
use proto::declaration_service_client::DeclarationServiceClient;

struct TestService {
    rest_base_url: String,
    grpc_addr: String,
    _postgres: ContainerAsync<Postgres>,
}

async fn spawn_service() -> TestService {
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
    let outbox_admin = Arc::new(OutboxAdminStore::new(pool.clone()));
    let idempotency = Arc::new(IdempotencyStore::new(pool));

    // Bind both REST and gRPC to ephemeral ports.
    let rest_listener = TcpListener::bind("127.0.0.1:0").expect("bind rest ephemeral");
    let rest_addr = rest_listener.local_addr().expect("rest local_addr");
    drop(rest_listener);
    let rest_bind_addr = format!("127.0.0.1:{}", rest_addr.port());

    let grpc_listener = TcpListener::bind("127.0.0.1:0").expect("bind grpc ephemeral");
    let grpc_addr_sock = grpc_listener.local_addr().expect("grpc local_addr");
    drop(grpc_listener);
    let grpc_bind_addr = format!("127.0.0.1:{}", grpc_addr_sock.port());

    let cfg = test_config(&rest_bind_addr, &database_url, &grpc_bind_addr);

    let app_state = AppState {
        submit_usecase: submit,
        get_usecase: get,
        record_verification_usecase: record_verification,
        supersede_usecase: supersede,
        amend_usecase: amend,
        correct_usecase: correct,
        idempotency,
        outbox_admin,
        base_url: format!("http://{rest_bind_addr}"),
        is_dev: true,
        idempotency_ttl_seconds: 3600,
        oidc: None,
        metrics: recor_declaration::metrics::Metrics::new().expect("metrics registry"),
    };

    // Spawn REST.
    let rest_router = recor_declaration::api::router(app_state.clone(), &cfg);
    let rest_tcp = tokio::net::TcpListener::bind(&rest_bind_addr)
        .await
        .expect("rest rebind");
    tokio::spawn(async move {
        axum::serve(rest_tcp, rest_router).await.expect("axum serve");
    });

    // Spawn gRPC.
    let grpc_auth = GrpcAuthConfig {
        is_dev: cfg.is_dev(),
        oidc: app_state.oidc.clone(),
    };
    let grpc_service =
        DeclarationGrpcService::new(app_state).into_server_with_auth(grpc_auth);
    let grpc_sock_addr: std::net::SocketAddr =
        grpc_bind_addr.parse().expect("parse grpc bind addr");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(grpc_service)
            .serve(grpc_sock_addr)
            .await
            .expect("tonic serve");
    });

    // Wait for REST to be ready.
    let client = reqwest::Client::new();
    for _ in 0..40 {
        if let Ok(resp) = client
            .get(&format!("http://{rest_bind_addr}/healthz"))
            .send()
            .await
        {
            if resp.status() == StatusCode::OK {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Wait briefly for gRPC; a successful TCP connect signals readiness.
    for _ in 0..40 {
        if std::net::TcpStream::connect(&grpc_bind_addr).is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    TestService {
        rest_base_url: format!("http://{rest_bind_addr}"),
        grpc_addr: grpc_bind_addr,
        _postgres: postgres_container,
    }
}

fn test_config(rest_bind_addr: &str, database_url: &str, grpc_bind_addr: &str) -> Config {
    use secrecy::SecretString;
    Config {
        bind_addr: rest_bind_addr.to_string(),
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
        // R-DECL-8: the gRPC harness sets this to the ephemeral
        // gRPC port; the actual server is spawned manually above
        // because we want to share `AppState` across both transports
        // and the harness already manages its own task lifecycle.
        grpc_bind_addr: grpc_bind_addr.to_string(),
    }
}

/// Build the canonical bytes the declarant signs. MUST stay
/// byte-parity with `api::rest::canonical_payload_bytes` (the REST
/// server checks against these bytes on incoming requests) AND with
/// `api::grpc::canonical_submit_bytes` (the gRPC server's verification
/// path). One of the doctrines this test exercises is D15: signatures
/// produced for either transport must verify against the other.
fn canonical_submit_string(
    entity_id: Uuid,
    principal: &str,
    person_id: Uuid,
    nonce_hex: &str,
) -> String {
    // Field order: entity_id, declarant_principal, declarant_role,
    // kind, effective_from, beneficial_owners, nonce_hex. Don't reach
    // for `serde_json::Value` here — it sorts keys alphabetically.
    format!(
        "{{\"entity_id\":\"{entity_id}\",\
\"declarant_principal\":\"{principal}\",\
\"declarant_role\":\"self\",\
\"kind\":\"incorporation\",\
\"effective_from\":\"2026-01-01\",\
\"beneficial_owners\":[{{\"person_id\":\"{person_id}\",\"ownership_basis_points\":10000,\"interest_kind\":\"equity\"}}],\
\"nonce_hex\":\"{nonce_hex}\"}}"
    )
}

fn build_attestation(
    key: &SigningKey,
    principal: &str,
    canonical_bytes: &[u8],
    nonce_hex: &str,
) -> proto::Attestation {
    let signature = key.sign(canonical_bytes);
    proto::Attestation {
        signed_by: principal.to_string(),
        signature_algorithm: "ed25519".to_string(),
        signature_hex: hex::encode(signature.to_bytes()),
        public_key_hex: hex::encode(key.verifying_key().to_bytes()),
        nonce_hex: nonce_hex.to_string(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker for testcontainers"]
async fn grpc_submit_then_rest_get_returns_same_data() {
    let svc = spawn_service().await;

    let key = SigningKey::from_bytes(&[77u8; 32]);
    let principal = "spiffe://recor.cm/grpc-integration-declarant";
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let person_id = Uuid::now_v7();
    let nonce_hex = hex::encode(Uuid::new_v4().as_bytes());

    let canonical = canonical_submit_string(entity_id, principal, person_id, &nonce_hex);
    let attestation = build_attestation(&key, principal, canonical.as_bytes(), &nonce_hex);

    // Connect via tonic. Dev-mode auth uses the X-Recor-Dev-Principal
    // metadata header, set on every outgoing request via the
    // `with_interceptor` builder.
    let dev_principal = principal.to_string();
    let endpoint = format!("http://{}", svc.grpc_addr);
    let channel = tonic::transport::Channel::from_shared(endpoint)
        .expect("parse endpoint")
        .connect()
        .await
        .expect("connect to grpc server");
    let mut client =
        DeclarationServiceClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
            req.metadata_mut().insert(
                "x-recor-dev-principal",
                dev_principal.parse().expect("metadata value"),
            );
            Ok(req)
        });

    let resp = client
        .submit_declaration(tonic::Request::new(proto::SubmitDeclarationRequest {
            declaration_id: declaration_id.to_string(),
            entity_id: entity_id.to_string(),
            declarant_role: proto::DeclarantRole::Self_ as i32,
            kind: proto::DeclarationKind::Incorporation as i32,
            effective_from: "2026-01-01".to_string(),
            beneficial_owners: vec![proto::BeneficialOwner {
                person_id: person_id.to_string(),
                ownership_basis_points: 10_000,
                interest_kind: proto::InterestKind::Equity as i32,
            }],
            attestation: Some(attestation),
        }))
        .await
        .expect("grpc submit ok");
    let submit_response = resp.into_inner();
    assert_eq!(submit_response.declaration_id, declaration_id.to_string());
    assert_eq!(submit_response.state, "submitted");
    assert_eq!(submit_response.receipt_hash_hex.len(), 64);

    // Query via REST GET and assert the same projection comes back.
    let http = reqwest::Client::new();
    let resp = http
        .get(format!(
            "{}/v1/declarations/{}",
            svc.rest_base_url, declaration_id
        ))
        .header("x-recor-dev-principal", principal)
        .send()
        .await
        .expect("rest get");
    assert_eq!(resp.status(), StatusCode::OK);
    let rest_body: Value = resp.json().await.expect("rest body");
    assert_eq!(
        rest_body["declaration_id"],
        Value::String(declaration_id.to_string())
    );
    assert_eq!(
        rest_body["declarant_principal"],
        Value::String(principal.to_string())
    );
    assert_eq!(rest_body["state"], Value::String("submitted".to_string()));
    assert_eq!(rest_body["aggregate_version"], serde_json::json!(1));
    assert_eq!(
        rest_body["entity_id"],
        Value::String(entity_id.to_string())
    );
    assert_eq!(
        rest_body["beneficial_owners"][0]["person_id"],
        Value::String(person_id.to_string())
    );
    assert_eq!(
        rest_body["beneficial_owners"][0]["ownership_basis_points"],
        serde_json::json!(10_000)
    );

    // The submit response's receipt_hash MUST equal the GET projection's.
    assert_eq!(
        rest_body["receipt_hash_hex"],
        Value::String(submit_response.receipt_hash_hex)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker for testcontainers"]
async fn grpc_unauthenticated_request_is_refused() {
    // D14 / D17: without any principal credential, the gRPC interceptor
    // refuses the request before it reaches the handler.
    let svc = spawn_service().await;
    let endpoint = format!("http://{}", svc.grpc_addr);
    let channel = tonic::transport::Channel::from_shared(endpoint)
        .expect("parse endpoint")
        .connect()
        .await
        .expect("connect");
    let mut client = DeclarationServiceClient::new(channel);

    let err = client
        .get_declaration(tonic::Request::new(proto::GetDeclarationRequest {
            declaration_id: Uuid::now_v7().to_string(),
        }))
        .await
        .expect_err("expected unauthenticated");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker for testcontainers"]
async fn grpc_malformed_uuid_is_invalid_argument() {
    let svc = spawn_service().await;
    let principal = "spiffe://recor.cm/grpc-validation".to_string();
    let endpoint = format!("http://{}", svc.grpc_addr);
    let channel = tonic::transport::Channel::from_shared(endpoint)
        .expect("parse endpoint")
        .connect()
        .await
        .expect("connect");
    let mut client =
        DeclarationServiceClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
            req.metadata_mut()
                .insert("x-recor-dev-principal", principal.parse().unwrap());
            Ok(req)
        });

    let err = client
        .get_declaration(tonic::Request::new(proto::GetDeclarationRequest {
            declaration_id: "not-a-uuid".to_string(),
        }))
        .await
        .expect_err("expected invalid argument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
