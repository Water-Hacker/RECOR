//! Integration tests — drive the CLI command handlers against a
//! wiremock-served fake platform. The tests assert wire-shape
//! correctness (URLs, headers, JSON payloads), not the binary's
//! `main` parsing path (the latter is covered by clap's own
//! invariants + the unit tests in `lib.rs` and `command.rs`).

use std::time::Duration;

use recor_cli::{command, http_client, CliConfig, Service};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg_for(server: &MockServer, token: Option<&str>) -> CliConfig {
    CliConfig::builder()
        .base_url(server.uri())
        .token(token.map(str::to_string))
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build cfg")
}

#[tokio::test]
async fn health_happy_path_returns_ok_summary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/verification-engine/healthz"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "ok", "db": "ok"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, None);
    let http = http_client(&cfg).unwrap();
    let out = command::health(&cfg, &http, Service::VerificationEngine)
        .await
        .expect("health succeeds");
    assert!(out.contains("verification-engine OK"));
    assert!(out.contains("\"db\""));
}

#[tokio::test]
async fn health_non_2xx_returns_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/declaration/healthz"))
        .respond_with(ResponseTemplate::new(503).set_body_string("db down"))
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, None);
    let http = http_client(&cfg).unwrap();
    let err = command::health(&cfg, &http, Service::Declaration)
        .await
        .unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("503"));
    assert!(msg.contains("db down"));
}

#[tokio::test]
async fn verify_pretty_prints_report() {
    let server = MockServer::start().await;
    let report = serde_json::json!({
        "declaration_id": "018f0000-0000-7000-8000-000000000001",
        "verified": true,
        "entries": [{"event_id": "e1", "receipt_hash_hex": "deadbeef"}]
    });
    Mock::given(method("GET"))
        .and(path(
            "/audit-verifier/v1/audit/verify/018f0000-0000-7000-8000-000000000001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(report.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, None);
    let http = http_client(&cfg).unwrap();
    let out = command::verify(&cfg, &http, "018f0000-0000-7000-8000-000000000001")
        .await
        .expect("verify succeeds");
    // Output is pretty-printed JSON — assert the keys + a sentinel
    // value are present rather than locking exact whitespace.
    assert!(out.contains("\"declaration_id\""));
    assert!(out.contains("\"verified\""));
    assert!(out.contains("deadbeef"));
}

#[tokio::test]
async fn sanctions_search_requires_token() {
    let server = MockServer::start().await;
    // The mock never gets hit — the CLI fails closed BEFORE sending.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, None);
    let http = http_client(&cfg).unwrap();
    let err = command::sanctions_search(&cfg, &http, "Smith")
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("admin token"));
}

#[tokio::test]
async fn sanctions_search_with_token_posts_name_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verification-engine/v1/internal/sanctions/search"))
        .and(header("authorization", "Bearer t0k3n"))
        .and(body_json(serde_json::json!({"name": "Smith"})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"id": "ofac/SDN-1234", "score": 0.92}],
                "elapsed_ms": 12
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, Some("t0k3n"));
    let http = http_client(&cfg).unwrap();
    let out = command::sanctions_search(&cfg, &http, "Smith")
        .await
        .expect("sanctions search succeeds");
    assert!(out.contains("\"hits\""));
    assert!(out.contains("SDN-1234"));
}

#[tokio::test]
async fn dlq_list_v_engine_uses_verification_outbox_dlq_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/verification-engine/v1/internal/verification-outbox-dlq",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total": 0,
                "limit": 50,
                "offset": 0,
                "items": []
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, Some("admin-token"));
    let http = http_client(&cfg).unwrap();
    let out = command::dlq_list(&cfg, &http, Service::VerificationEngine)
        .await
        .expect("dlq list succeeds");
    assert!(out.contains("\"items\""));
    assert!(out.contains("\"total\""));
}

#[tokio::test]
async fn dlq_replay_declaration_uses_correct_route() {
    let server = MockServer::start().await;
    let row_id = "018f0000-0000-7000-8000-00000000abcd";
    Mock::given(method("POST"))
        .and(path(format!(
            "/declaration/v1/internal/outbox-dlq/{row_id}/replay"
        )))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "replayed_id": row_id,
                "queued": true
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let cfg = cfg_for(&server, Some("admin-token"));
    let http = http_client(&cfg).unwrap();
    let out = command::dlq_replay(&cfg, &http, Service::Declaration, row_id)
        .await
        .expect("dlq replay succeeds");
    assert!(out.contains(row_id));
    assert!(out.contains("queued"));
}
