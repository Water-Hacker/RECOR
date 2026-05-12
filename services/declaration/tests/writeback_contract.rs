//! Contract test for the V→D writeback envelope.
//!
//! Resolves [R-DECL-10](https://github.com/Water-Hacker/RECOR/issues/42).
//!
//! The Verification Engine's outbox-relay POSTs `verification.completed.v1`
//! envelopes to the Declaration service's `/v1/internal/verification-outcomes`
//! endpoint. The wire format is produced by
//! `services/verification-engine/src/infrastructure/postgres.rs` and consumed
//! by `services/declaration/src/api/dto.rs::VerificationOutcomeRequest`.
//!
//! If either side renames a field or changes a type, this test fails. The
//! fixture below is the **canonical contract** — any change here is a
//! deliberate breaking change to the cross-service interface and must be
//! reviewed.

use serde_json::json;
use uuid::Uuid;

use recor_declaration::api::dto::VerificationOutcomeRequest;
use recor_declaration::domain::VerificationLane;

/// The exact envelope shape produced by the verification engine's outbox.
/// Field order matches `services/verification-engine/src/infrastructure/
/// postgres.rs` writeback_payload construction (see commit history for
/// the canonical source).
fn canonical_writeback_envelope() -> serde_json::Value {
    json!({
        "case_id":                          "019e1a00-0000-7000-8000-000000000001",
        "declaration_id":                   "00000000-0000-4000-8000-000000000abc",
        "lane":                             "green",
        "fused_authenticity_belief":        0.92,
        "fused_authenticity_plausibility":  0.97,
        "fused_risk_belief":                0.05,
        "completed_at":                     "2026-05-12T01:30:46.123456789Z"
    })
}

#[test]
fn green_lane_envelope_round_trips() {
    let envelope = canonical_writeback_envelope();
    let outcome: VerificationOutcomeRequest = serde_json::from_value(envelope.clone())
        .expect("declaration service must accept the verification engine's payload");

    assert_eq!(
        outcome.case_id,
        Uuid::parse_str("019e1a00-0000-7000-8000-000000000001").unwrap()
    );
    assert_eq!(
        outcome.declaration_id.0,
        Uuid::parse_str("00000000-0000-4000-8000-000000000abc").unwrap()
    );
    assert_eq!(outcome.lane, VerificationLane::Green);
    assert!((outcome.fused_authenticity_belief - 0.92).abs() < 1e-9);
    assert!((outcome.fused_authenticity_plausibility - 0.97).abs() < 1e-9);
    assert!((outcome.fused_risk_belief - 0.05).abs() < 1e-9);
    // completed_at is RFC3339 — confirm parsed into an OffsetDateTime
    // by serialising back and checking the date stayed.
    let back = serde_json::to_value(&outcome).expect("serialise back");
    assert_eq!(
        back.get("completed_at").and_then(|v| v.as_str()).unwrap_or(""),
        "2026-05-12T01:30:46.123456789Z"
    );
}

#[test]
fn yellow_and_red_lanes_round_trip() {
    for lane_str in ["yellow", "red"] {
        let mut envelope = canonical_writeback_envelope();
        envelope["lane"] = serde_json::Value::String(lane_str.to_string());
        let outcome: VerificationOutcomeRequest =
            serde_json::from_value(envelope).expect("lane variant must parse");
        match (lane_str, outcome.lane) {
            ("yellow", VerificationLane::Yellow) | ("red", VerificationLane::Red) => {}
            (other, parsed) => panic!("lane {other} parsed as {parsed:?}"),
        }
    }
}

#[test]
fn unknown_lane_value_rejects() {
    let mut envelope = canonical_writeback_envelope();
    envelope["lane"] = serde_json::Value::String("purple".to_string());
    let result: Result<VerificationOutcomeRequest, _> = serde_json::from_value(envelope);
    assert!(result.is_err(), "unknown lane variants must NOT silently parse");
}

#[test]
fn missing_required_field_rejects() {
    for field in [
        "case_id",
        "declaration_id",
        "lane",
        "fused_authenticity_belief",
        "fused_authenticity_plausibility",
        "fused_risk_belief",
        "completed_at",
    ] {
        let mut envelope = canonical_writeback_envelope();
        envelope
            .as_object_mut()
            .expect("object")
            .remove(field);
        let result: Result<VerificationOutcomeRequest, _> =
            serde_json::from_value(envelope);
        assert!(
            result.is_err(),
            "envelope missing {field} must NOT parse; the V-engine MUST send this field"
        );
    }
}

#[test]
fn non_rfc3339_completed_at_rejects() {
    let mut envelope = canonical_writeback_envelope();
    // 9-element array — the OLD wire format before iso_datetime
    // serializer landed. Asserts we never silently fall back.
    envelope["completed_at"] = json!([2026, 132, 1, 30, 46, 0, 0, 0, 0]);
    let result: Result<VerificationOutcomeRequest, _> = serde_json::from_value(envelope);
    assert!(
        result.is_err(),
        "9-element-array timestamp must NOT parse — this is the bug the iso_datetime annotation fixed"
    );
}

#[test]
fn belief_value_passes_through_full_precision() {
    let mut envelope = canonical_writeback_envelope();
    envelope["fused_authenticity_belief"] =
        serde_json::Value::Number(serde_json::Number::from_f64(0.123456789012345).unwrap());
    let outcome: VerificationOutcomeRequest = serde_json::from_value(envelope).unwrap();
    assert!(
        (outcome.fused_authenticity_belief - 0.123456789012345).abs() < 1e-15,
        "f64 precision must round-trip — beliefs feed the lane router downstream"
    );
}
