//! End-to-end OIDC integration test.
//!
//! Spawns a tiny in-process mock OIDC issuer that:
//!   - serves a fixed `/.well-known/openid-configuration` document
//!     pointing at its own `/jwks` endpoint
//!   - serves the test RSA public key as a JWKS at `/jwks`
//!
//! Constructs an `OidcVerifier` against the mock, signs a token with
//! the test private key, and verifies it. This exercises the full
//! discovery → JWKS fetch → signature verify → claim validate path
//! without requiring an external OIDC provider.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Json, Router};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use time::OffsetDateTime;

use recor_declaration::api::OidcVerifier;

const TEST_AUDIENCE: &str = "recor-test";
const TEST_KID: &str = "test-key-1";

fn rsa_signing_key() -> EncodingKey {
    const PEM: &str = include_str!("./fixtures/test_rsa_pkcs8.pem");
    EncodingKey::from_rsa_pem(PEM.as_bytes()).expect("test RSA PEM")
}

fn rsa_jwk() -> serde_json::Value {
    let raw = include_str!("./fixtures/test_rsa_jwk.json");
    serde_json::from_str(raw).expect("test JWK JSON")
}

async fn spawn_mock_issuer() -> String {
    // Bind an ephemeral port.
    let std_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = std_listener.local_addr().expect("local_addr");
    drop(std_listener);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    let issuer_clone = issuer.clone();

    let jwks_response = json!({ "keys": [rsa_jwk()] });
    let jwks_response_for_jwks = jwks_response.clone();

    let router = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || {
                let issuer = issuer_clone.clone();
                async move {
                    Json(json!({
                        "issuer": issuer,
                        "jwks_uri": format!("{issuer}/jwks"),
                    }))
                }
            }),
        )
        .route(
            "/jwks",
            get(move || {
                let body = jwks_response_for_jwks.clone();
                async move { Json(body) }
            }),
        );

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind tokio");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    // Allow the server a moment to start accepting connections.
    tokio::time::sleep(Duration::from_millis(80)).await;
    issuer
}

fn sign_token(issuer: &str, sub: &str, exp_offset_seconds: i64) -> String {
    let key = rsa_signing_key();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let claims = json!({
        "iss": issuer,
        "aud": TEST_AUDIENCE,
        "sub": sub,
        "exp": OffsetDateTime::now_utc().unix_timestamp() + exp_offset_seconds,
        "iat": OffsetDateTime::now_utc().unix_timestamp() - 5,
    });
    encode(&header, &claims, &key).expect("encode")
}

#[tokio::test]
async fn discovery_and_verification_e2e() {
    let issuer = spawn_mock_issuer().await;
    let verifier: Arc<OidcVerifier> = OidcVerifier::discover(&issuer, TEST_AUDIENCE)
        .await
        .expect("discover the mock issuer");

    let token = sign_token(&issuer, "spiffe://recor.cm/declarant-42", 300);
    let claims = verifier.verify(&token).await.expect("verify");
    assert_eq!(claims.sub, "spiffe://recor.cm/declarant-42");
    assert_eq!(claims.iss, issuer);
}

#[tokio::test]
async fn token_signed_by_different_issuer_rejects() {
    let issuer = spawn_mock_issuer().await;
    let verifier: Arc<OidcVerifier> =
        OidcVerifier::discover(&issuer, TEST_AUDIENCE).await.unwrap();

    // Sign a token with the right key but claiming a different issuer.
    let token = sign_token("https://attacker.example", "x", 300);
    let err = verifier.verify(&token).await.unwrap_err();
    // Issuer mismatch surfaces as TokenInvalid (jsonwebtoken's
    // claim-validation error).
    let msg = format!("{err}");
    assert!(
        msg.contains("token decode") || msg.contains("invalid"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn discovery_against_unreachable_issuer_errors() {
    // Loopback address that nothing's listening on.
    let result = OidcVerifier::discover(
        "http://127.0.0.1:1/will-not-resolve",
        TEST_AUDIENCE,
    )
    .await;
    assert!(result.is_err(), "discovery should fail against a dead issuer");
}
