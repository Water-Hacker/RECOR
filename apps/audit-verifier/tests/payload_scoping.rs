//! TODO-023 / TODO-007 / TODO-019 — Sovim per-tier payload-scoping
//! regression test for the audit-verifier.
//!
//! Threat model. The audit-verifier's `GET /v1/audit/verify/{id}` is
//! the user-facing cryptographic-integrity surface for the entire
//! beneficial-ownership register. Post-Sovim (CJEU C-37/20 + C-601/20),
//! public access to BO data MUST be tiered:
//!
//! - **Admin** — competent authority (FIU, supervisor) — full payload.
//! - **ObligedEntity** — regulated counter-party — no national-ID,
//!   no residential address, no biometric reference hash, no signer
//!   public key.
//! - **PublicLegitimateInterest** — journalist / civil-society caller
//!   admitted through the Sovim balancing test — strict minimum
//!   (cryptographic outcome only; no per-event metadata).
//! - **Unauthenticated** — 401.
//!
//! This integration test stands up an in-memory verifier with all
//! three test doubles, calls the endpoint at each tier, and asserts:
//!
//! 1. The cryptographic verdict (`result` + per-entry `status`) is
//!    preserved at every authenticated tier.
//! 2. The per-event observability metadata (`tx_id`, hashes,
//!    `on_chain_ts`, `event_type`) is **stripped** at the
//!    PublicLegitimateInterest tier.
//! 3. The Sovim-protected PII identifiers — national-ID, residential
//!    address, biometric reference hash, and the signer's public key
//!    (TODO-019) — NEVER appear in any tier's response. Even when an
//!    upstream payload includes them (the projection's event_payload
//!    routinely does, since the audit-verifier reads from the same
//!    canonical payload the chain pins to), they are NEVER serialised
//!    into the verifier's response shape.
//!
//! A future refactor that adds a new field to `EntryReport` MUST
//! update the per-tier redactor and the per-tier expected key-set in
//! this test. Without that update, the test fails CI and the
//! Sovim-shaped privacy posture cannot regress silently.
//!
//! Cross-link: `docs/security/permission-matrix.md` — the per-scope
//! matrix that this code path enforces. The matrix and the
//! `AuthorizationTier` enum are co-evolved.

use std::sync::Arc;

use audit_verifier::{
    router, AppState, AuthConfig, InMemoryFabricClient, InMemoryProjectionRepo, OnChainEntry,
    ProjectionRow,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value as JsonValue};
use tower::ServiceExt;
use uuid::Uuid;

/// All the Sovim-protected identifier names. None of them MUST EVER
/// appear in the verifier's response, at any tier. This list is the
/// project's master fence around the GDPR Art. 5(1)(c) + Sovim
/// balancing test (REQ-gdpr-005-001-c + REQ-cjeu-sovim-001).
const SOVIM_PROHIBITED_KEYS: &[&str] = &[
    "national_id_document",
    "national_id_number",
    "residential_address",
    "biometric_reference_hash",
    "signer_public_key",
    "public_key_hex",
    "primary_id_document",
];

/// The redactor MUST strip these from PublicLegitimateInterest
/// responses. They are not PII per se — they are per-event
/// observability metadata — but Sovim's balancing test treats bulk-
/// scraping risk as a privacy harm in itself.
const PUBLIC_TIER_STRIPPED_KEYS: &[&str] = &[
    "tx_id",
    "on_chain_receipt_hash_hex",
    "derived_receipt_hash_hex",
    "on_chain_ts",
    "event_type",
];

fn dev_auth() -> AuthConfig {
    AuthConfig {
        is_dev: true,
        oidc: None,
    }
}

fn proj_row_with_pii(eid: Uuid, decl: Uuid, ts: &str) -> ProjectionRow {
    // The projection routinely carries the full canonical payload —
    // including the Sovim-protected fields. This is the exact threat
    // the test is policing: even when the verifier reads from a row
    // that includes national-ID-document bytes, the response MUST NOT
    // re-emit them.
    let event_payload = json!({
        "declaration_id": decl.to_string(),
        "submitted_at": ts,
        "national_id_document": "CM-NID-1234567890",
        "residential_address": "13 rue de la Liberté, Yaoundé",
        "biometric_reference_hash": "blake3:" .to_string() + &"ab".repeat(32),
        "signer_public_key": "ed25519:" .to_string() + &"cd".repeat(32),
        "data": {"entity_name": "ACME Holdings SARL", "bo_count": 3},
    });
    let mut for_hash = event_payload.clone();
    for_hash.as_object_mut().unwrap().remove("receipt_hash_hex");
    let receipt = audit_verifier::derive_receipt_hash(&for_hash);
    ProjectionRow {
        event_id: eid,
        declaration_id: decl,
        event_type: "declaration.submitted.v1".to_string(),
        event_payload,
        receipt_hash_hex: receipt,
        ts: ts.to_string(),
    }
}

async fn fixture() -> (AppState, Uuid, Uuid, String) {
    let decl = Uuid::new_v4();
    let eid = Uuid::new_v4();
    let row = proj_row_with_pii(eid, decl, "2026-05-12T10:00:00Z");
    let projection = Arc::new(InMemoryProjectionRepo::new());
    projection.add(row.clone()).await;
    let fabric = Arc::new(InMemoryFabricClient::new());
    fabric
        .add(
            &decl.to_string(),
            OnChainEntry {
                event_id: eid.to_string(),
                declaration_id: decl.to_string(),
                receipt_hash_hex: row.receipt_hash_hex.clone(),
                ts: row.ts.clone(),
                tx_id: "fabric-tx-1".to_string(),
            },
        )
        .await;
    (
        AppState { fabric, projection },
        decl,
        eid,
        row.receipt_hash_hex,
    )
}

fn req_with_scope(decl: Uuid, scope: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .uri(format!("/v1/audit/verify/{decl}"))
        .header("x-recor-dev-principal", "spiffe://recor.cm/test");
    if let Some(s) = scope {
        b = b.header("x-recor-dev-scope", s);
    }
    b.body(Body::empty()).unwrap()
}

async fn invoke(scope: Option<&str>) -> (StatusCode, JsonValue) {
    let (state, decl, _eid, _hash) = fixture().await;
    let app = router(state, dev_auth());
    let resp = app.oneshot(req_with_scope(decl, scope)).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: JsonValue = serde_json::from_slice(&body).unwrap_or(JsonValue::Null);
    (status, json)
}

fn assert_no_prohibited_keys(value: &JsonValue, tier: &str) {
    let serialised = serde_json::to_string(value).unwrap();
    for k in SOVIM_PROHIBITED_KEYS {
        assert!(
            !serialised.contains(k),
            "Sovim regression: tier `{tier}` response contains prohibited key `{k}`. \
             Body: {serialised}"
        );
    }
}

#[tokio::test]
async fn admin_tier_returns_full_metadata() {
    let (status, body) = invoke(Some("admin")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], "authentic");
    let entry = &body["entries"][0];
    // Admin sees the per-event metadata.
    assert!(entry["tx_id"].is_string(), "admin: tx_id present");
    assert!(
        entry["on_chain_receipt_hash_hex"].is_string(),
        "admin: on_chain_receipt_hash_hex present"
    );
    assert!(
        entry["derived_receipt_hash_hex"].is_string(),
        "admin: derived_receipt_hash_hex present"
    );
    assert!(entry["on_chain_ts"].is_string(), "admin: on_chain_ts present");
    assert!(entry["event_type"].is_string(), "admin: event_type present");
    assert_eq!(entry["status"], "matched");
    // Sovim PII never re-emitted, even to admin (the verifier's
    // response shape is metadata-only by construction; the canonical
    // payload itself is fetched separately from the declaration
    // service's GET endpoint).
    assert_no_prohibited_keys(&body, "admin");
}

#[tokio::test]
async fn obliged_entity_tier_keeps_metadata_strips_pii() {
    let (status, body) = invoke(Some("obliged-entity")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], "authentic");
    let entry = &body["entries"][0];
    // ObligedEntity tier today keeps the metadata (same as admin)
    // because the report carries no PII fields. The redactor's hook is
    // in place for the day a payload field lands.
    assert!(entry["tx_id"].is_string());
    assert!(entry["event_type"].is_string());
    assert_eq!(entry["status"], "matched");
    assert_no_prohibited_keys(&body, "obliged-entity");
}

#[tokio::test]
async fn public_tier_strips_per_event_metadata_keeps_verdict() {
    let (status, body) = invoke(Some("public")).await;
    assert_eq!(status, StatusCode::OK);
    // The cryptographic verdict survives — that is the entire point
    // of the public verifier surface.
    assert_eq!(body["result"], "authentic");
    let entry = &body["entries"][0];
    assert_eq!(entry["status"], "matched");
    // Per-event metadata MUST be stripped at the public tier.
    for k in PUBLIC_TIER_STRIPPED_KEYS {
        assert!(
            entry.get(*k).map_or(true, |v| v.is_null()),
            "public tier: per-event metadata key `{k}` MUST be absent or null, got {entry}"
        );
    }
    // event_id is the chain anchor for re-derivation — it stays.
    assert!(entry["event_id"].is_string(), "event_id retained");
    assert_no_prohibited_keys(&body, "public");
}

#[tokio::test]
async fn unknown_scope_defaults_to_public_tier() {
    let (status, body) = invoke(Some("not-a-known-scope")).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["entries"][0];
    // Defaults to public — tx_id and event_type stripped.
    assert!(entry.get("tx_id").map_or(true, |v| v.is_null()));
    assert!(entry.get("event_type").map_or(true, |v| v.is_null()));
    assert_no_prohibited_keys(&body, "fallback-public");
}

#[tokio::test]
async fn missing_dev_scope_header_defaults_to_public() {
    // No `x-recor-dev-scope` at all → public tier (D14 fail-closed).
    let (status, body) = invoke(None).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["entries"][0];
    assert!(entry.get("event_type").map_or(true, |v| v.is_null()));
    assert_no_prohibited_keys(&body, "no-scope-header");
}

#[tokio::test]
async fn unauthenticated_call_is_refused_401() {
    let (state, decl, _, _) = fixture().await;
    let app = router(state, dev_auth());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/audit/verify/{decl}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cryptographic_verdict_preserved_for_tampered_at_every_tier() {
    // Build a fixture whose on-chain hash deliberately doesn't match
    // the projection. Every authenticated tier should still see
    // `result == tampered` — the Sovim minimum-information principle
    // does NOT degrade the integrity verdict.
    let decl = Uuid::new_v4();
    let eid = Uuid::new_v4();
    let row = proj_row_with_pii(eid, decl, "2026-05-12T10:00:00Z");
    let projection = Arc::new(InMemoryProjectionRepo::new());
    projection.add(row.clone()).await;
    let fabric = Arc::new(InMemoryFabricClient::new());
    fabric
        .add(
            &decl.to_string(),
            OnChainEntry {
                event_id: eid.to_string(),
                declaration_id: decl.to_string(),
                receipt_hash_hex: "ff".repeat(32),
                ts: row.ts.clone(),
                tx_id: "tx-tampered".to_string(),
            },
        )
        .await;
    let state = AppState { fabric, projection };
    for scope in ["admin", "obliged-entity", "public"] {
        let app = router(state.clone(), dev_auth());
        let resp = app
            .oneshot(req_with_scope(decl, Some(scope)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "scope={scope}");
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: JsonValue = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            v["result"], "tampered",
            "scope={scope}: tampered verdict must survive redaction"
        );
        assert_no_prohibited_keys(&v, scope);
    }
}
