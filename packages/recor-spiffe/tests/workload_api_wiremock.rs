//! Integration tests: drive the [`HttpWorkloadApi`] against a
//! wiremock-backed SPIRE Workload API stub.
//!
//! These tests are NOT a substitute for the production gRPC client —
//! they assert that the HTTP-shaped trait impl + the [`SpiffeClient`]
//! bootstrap path handle the contract correctly (success, 404,
//! malformed JSON, missing trust bundle, etc).

use std::sync::Arc;

use recor_spiffe::{
    HttpWorkloadApi, SpiffeClient, SpiffeError, SpiffeMetrics, X509SvidResponse,
};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper: encode the SVID response as JSON in the shape
/// [`HttpWorkloadApi::fetch_svid`] expects.
fn svid_json(
    spiffe_id: &str,
    chain_pem: &str,
    key_pem: &str,
    trust_bundle_pem: &str,
) -> serde_json::Value {
    json!({
        "spiffe_id": spiffe_id,
        "chain_pem": chain_pem,
        "key_pem": key_pem,
        "trust_bundle_pem": trust_bundle_pem,
    })
}

const DUMMY_CERT: &str =
    "-----BEGIN CERTIFICATE-----\nMIIBADCBu6ADAgECAgEBMA==\n-----END CERTIFICATE-----\n";
const DUMMY_KEY: &str =
    "-----BEGIN PRIVATE KEY-----\nMIIBADCBu6ADAgECAgEBMA==\n-----END PRIVATE KEY-----\n";

#[tokio::test]
async fn bootstrap_succeeds_when_workload_api_returns_matching_svid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/svid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(svid_json(
            "spiffe://recor.cm/declaration",
            DUMMY_CERT,
            DUMMY_KEY,
            DUMMY_CERT,
        )))
        .mount(&server)
        .await;

    let api = HttpWorkloadApi::new(server.uri());
    let registry = prometheus::Registry::new();
    let metrics = Arc::new(SpiffeMetrics::register(&registry).unwrap());
    let client = SpiffeClient::new(Arc::new(api), Some(metrics));

    let bundle = client
        .bootstrap("spiffe://recor.cm/declaration")
        .await
        .expect("bootstrap succeeds against happy-path stub");
    assert_eq!(bundle.spiffe_id, "spiffe://recor.cm/declaration");
    assert!(!bundle.chain_pem.is_empty());
}

#[tokio::test]
async fn bootstrap_fails_when_workload_api_returns_different_spiffe_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/svid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(svid_json(
            "spiffe://recor.cm/portal",
            DUMMY_CERT,
            DUMMY_KEY,
            DUMMY_CERT,
        )))
        .mount(&server)
        .await;

    let api = HttpWorkloadApi::new(server.uri());
    let client = SpiffeClient::new(Arc::new(api), None);

    let r = client.bootstrap("spiffe://recor.cm/declaration").await;
    match r {
        Err(SpiffeError::SpiffeIdMismatch { expected, actual }) => {
            assert_eq!(expected, "spiffe://recor.cm/declaration");
            assert_eq!(actual, "spiffe://recor.cm/portal");
        }
        other => panic!("expected SpiffeIdMismatch; got {other:?}"),
    }
}

#[tokio::test]
async fn bootstrap_fails_on_malformed_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/svid"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .mount(&server)
        .await;

    let api = HttpWorkloadApi::new(server.uri());
    let client = SpiffeClient::new(Arc::new(api), None);

    let r = client.bootstrap("spiffe://recor.cm/declaration").await;
    assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
}

#[tokio::test]
async fn bootstrap_fails_on_empty_trust_bundle() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/svid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(svid_json(
            "spiffe://recor.cm/declaration",
            DUMMY_CERT,
            DUMMY_KEY,
            "", // empty trust bundle → fail-closed
        )))
        .mount(&server)
        .await;

    let api = HttpWorkloadApi::new(server.uri());
    let client = SpiffeClient::new(Arc::new(api), None);

    let r = client.bootstrap("spiffe://recor.cm/declaration").await;
    assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
}

#[tokio::test]
async fn bootstrap_increments_failure_metric_on_unreachable() {
    // No mock; the wiremock server is started but doesn't match `/api/v1/svid`,
    // so the connection succeeds but the response is 404. The HTTP shim
    // surfaces that as an error path through json-deserialise (404 body
    // is not a valid X509SvidResponse).
    let server = MockServer::start().await;
    let api = HttpWorkloadApi::new(server.uri());
    let registry = prometheus::Registry::new();
    let metrics = Arc::new(SpiffeMetrics::register(&registry).unwrap());
    let client = SpiffeClient::new(Arc::new(api), Some(metrics));

    let _ = client.bootstrap("spiffe://recor.cm/declaration").await;
    // Failure counter must have ticked at least once.
    let families = registry.gather();
    let counter = families
        .iter()
        .find(|f| f.name() == "recor_spiffe_svid_fetch_total")
        .expect("counter present");
    let failure_count: u64 = counter
        .get_metric()
        .iter()
        .filter(|m| {
            m.get_label()
                .iter()
                .any(|l| l.name() == "result" && l.value() == "failure")
        })
        .map(|m| m.get_counter().value() as u64)
        .sum();
    assert!(
        failure_count >= 1,
        "expected at least one failure-labelled tick; got {failure_count}"
    );
}

#[tokio::test]
async fn bootstrap_caches_the_bundle_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/svid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(svid_json(
            "spiffe://recor.cm/verification",
            DUMMY_CERT,
            DUMMY_KEY,
            DUMMY_CERT,
        )))
        .expect(1) // assert: bootstrap is the only thing that talks to the API
        .mount(&server)
        .await;

    let api = HttpWorkloadApi::new(server.uri());
    let client = SpiffeClient::new(Arc::new(api), None);

    let _ = client
        .bootstrap("spiffe://recor.cm/verification")
        .await
        .expect("first bootstrap succeeds");

    // Reading the cache should not re-call the API.
    let current = client.current().await.expect("cache populated");
    assert_eq!(current.spiffe_id, "spiffe://recor.cm/verification");
    // Drop the server — wiremock verifies the expect(1) on drop.
}

/// Round-trip an `X509SvidResponse` through serde to assert the
/// JSON contract is stable. Not a wiremock test per se, but lives
/// here because it documents the same wire shape.
#[test]
fn x509_svid_response_json_round_trip() {
    let original = X509SvidResponse {
        spiffe_id: "spiffe://recor.cm/declaration".into(),
        chain_pem: b"chain".to_vec(),
        key_pem: b"key".to_vec(),
        trust_bundle_pem: b"bundle".to_vec(),
    };
    let j = serde_json::to_string(&original).unwrap();
    let parsed: X509SvidResponse = serde_json::from_str(&j).unwrap();
    assert_eq!(parsed.spiffe_id, original.spiffe_id);
    assert_eq!(parsed.chain_pem, original.chain_pem);
    assert_eq!(parsed.key_pem, original.key_pem);
    assert_eq!(parsed.trust_bundle_pem, original.trust_bundle_pem);
}
