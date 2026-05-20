//! Verification report — the JSON the API returns to the caller.
//!
//! Per-entry status enum:
//!
//! - **Matched**: on-chain hash equals the re-derived hash.
//! - **Mismatch**: hashes differ → either the projection has been
//!   tampered with or the chaincode entry was anchored with the wrong
//!   hash. The report does not take a side; operators investigate.
//! - **MissingProjection**: the chaincode has an entry the projection
//!   doesn't — possibly the projection was rebuilt from a partial
//!   backup or the row was illicitly deleted (the COMP-2 triggers
//!   make this hard, but not impossible to an attacker with raw
//!   superuser).
//! - **MissingOnChain**: the projection has an event the chaincode
//!   doesn't — the bridge worker failed to anchor it (look in
//!   fabric_bridge_dlq).
//!
//! Top-level report verdict:
//!
//! - **Authentic**: every entry matched and the on-chain set is a
//!   superset of (or equal to) the projection set.
//! - **Tampered**: at least one Mismatch.
//! - **Incomplete**: no Mismatches but at least one MissingOnChain
//!   (the chain is behind the projection — recoverable, but the
//!   declaration cannot be considered fully anchored).
//! - **Unverifiable**: Fabric unreachable; verifier can't speak to the
//!   trust anchor. Returns 503 at the HTTP layer.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::auth::AuthorizationTier;
use crate::fabric_client::OnChainEntry;
use crate::hashing::derive_receipt_hash;
use crate::projection::ProjectionRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Matched,
    Mismatch,
    MissingProjection,
    MissingOnChain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult {
    Authentic,
    Tampered,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryReport {
    pub event_id: String,
    pub status: EntryStatus,
    /// The on-chain Fabric transaction id (when known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
    /// The on-chain receipt hash (when an on-chain entry exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_chain_receipt_hash_hex: Option<String>,
    /// The hash re-derived from the projection (when projection exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_receipt_hash_hex: Option<String>,
    /// The on-chain timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_chain_ts: Option<String>,
    /// The event type from the projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReport {
    pub declaration_id: String,
    pub result: VerificationResult,
    pub entries: Vec<EntryReport>,
    /// Total entries on the chain.
    pub on_chain_count: usize,
    /// Total events in the projection that map to this declaration_id.
    pub projection_count: usize,
}

impl VerificationReport {
    /// Apply Sovim-driven per-tier redaction (TODO-007 / TODO-023).
    ///
    /// The cryptographic verification outcome — `result` and per-entry
    /// `status` — is always preserved: it is the audit-verifier's load-
    /// bearing semantic and reveals no PII. The metadata fields that
    /// expose per-event observability (`tx_id`, `on_chain_ts`,
    /// `event_type`, hashes) are tiered as follows.
    ///
    /// - **Admin** — full payload; nothing redacted. Competent-
    ///   authority access (REQ-fatf-c24-008-fn-27).
    /// - **ObligedEntity** — keeps event_type, hashes, timestamps,
    ///   tx_id. National-ID, residential address, biometric hash,
    ///   signer_public_key — when ever added to the response — MUST be
    ///   blank at this tier (REQ-amld-iv-005). Today the report carries
    ///   none of those fields; the test in
    ///   `tests/payload_scoping.rs` asserts the field set remains
    ///   bounded so a future regression fails CI.
    /// - **PublicLegitimateInterest** — strict minimum: per-entry
    ///   status and the rolled-up `result` only. tx_id, hashes, times,
    ///   and event types are stripped (REQ-cjeu-sovim-006). This makes
    ///   bulk-scraping of per-event metadata useless.
    pub fn redact_for_tier(&mut self, tier: AuthorizationTier) {
        match tier {
            AuthorizationTier::Admin => {}
            AuthorizationTier::ObligedEntity => {
                // Reserved hook: when payload fields exist on
                // EntryReport in the future, strip the PII/biometric/
                // signer fields here. The integration test in
                // `tests/payload_scoping.rs` already asserts the
                // expected field-set per tier; adding a field without
                // updating the redactor will fail CI.
            }
            AuthorizationTier::PublicLegitimateInterest => {
                for e in &mut self.entries {
                    e.tx_id = None;
                    e.on_chain_receipt_hash_hex = None;
                    e.derived_receipt_hash_hex = None;
                    e.on_chain_ts = None;
                    e.event_type = None;
                }
            }
        }
    }
}

/// Build a verification report by joining on-chain entries with
/// projection rows. The projection rows are passed as a slice keyed by
/// event_id; the caller has already fetched them.
pub fn build_report(
    declaration_id: Uuid,
    on_chain: Vec<OnChainEntry>,
    projection: Vec<ProjectionRow>,
) -> VerificationReport {
    let on_chain_count = on_chain.len();
    let projection_count = projection.len();

    let mut proj_by_id: BTreeMap<String, ProjectionRow> = BTreeMap::new();
    let mut proj_ids: HashSet<String> = HashSet::new();
    for row in projection {
        let id = row.event_id.to_string();
        proj_ids.insert(id.clone());
        proj_by_id.insert(id, row);
    }

    let mut entries: Vec<EntryReport> = Vec::with_capacity(on_chain_count.max(projection_count));
    let mut seen_on_chain: HashSet<String> = HashSet::new();

    for ocl in &on_chain {
        seen_on_chain.insert(ocl.event_id.clone());
        match proj_by_id.get(&ocl.event_id) {
            Some(proj) => {
                let derived = derive_hash_from_event(&proj.event_payload);
                let status = if derived == ocl.receipt_hash_hex {
                    EntryStatus::Matched
                } else {
                    EntryStatus::Mismatch
                };
                entries.push(EntryReport {
                    event_id: ocl.event_id.clone(),
                    status,
                    tx_id: Some(ocl.tx_id.clone()),
                    on_chain_receipt_hash_hex: Some(ocl.receipt_hash_hex.clone()),
                    derived_receipt_hash_hex: Some(derived),
                    on_chain_ts: Some(ocl.ts.clone()),
                    event_type: Some(proj.event_type.clone()),
                });
            }
            None => {
                entries.push(EntryReport {
                    event_id: ocl.event_id.clone(),
                    status: EntryStatus::MissingProjection,
                    tx_id: Some(ocl.tx_id.clone()),
                    on_chain_receipt_hash_hex: Some(ocl.receipt_hash_hex.clone()),
                    derived_receipt_hash_hex: None,
                    on_chain_ts: Some(ocl.ts.clone()),
                    event_type: None,
                });
            }
        }
    }

    // Projection events with no on-chain entry — the bridge didn't anchor them.
    for (id, proj) in &proj_by_id {
        if !seen_on_chain.contains(id) {
            entries.push(EntryReport {
                event_id: id.clone(),
                status: EntryStatus::MissingOnChain,
                tx_id: None,
                on_chain_receipt_hash_hex: None,
                derived_receipt_hash_hex: Some(derive_hash_from_event(&proj.event_payload)),
                on_chain_ts: None,
                event_type: Some(proj.event_type.clone()),
            });
        }
    }

    // Sort by event_id ascending for deterministic JSON.
    entries.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    let result = roll_up(&entries);
    VerificationReport {
        declaration_id: declaration_id.to_string(),
        result,
        entries,
        on_chain_count,
        projection_count,
    }
}

fn roll_up(entries: &[EntryReport]) -> VerificationResult {
    let mut tampered = false;
    let mut incomplete = false;
    for e in entries {
        match e.status {
            EntryStatus::Mismatch | EntryStatus::MissingProjection => tampered = true,
            EntryStatus::MissingOnChain => incomplete = true,
            EntryStatus::Matched => {}
        }
    }
    if tampered {
        VerificationResult::Tampered
    } else if incomplete {
        VerificationResult::Incomplete
    } else {
        VerificationResult::Authentic
    }
}

/// Re-derive the receipt hash for a stored event. Strips the
/// `receipt_hash_hex` field from the payload before hashing (the
/// declaration service hashes the canonical body, NOT the body-with-
/// hash, since the hash is the OUTPUT of the operation).
fn derive_hash_from_event(payload: &JsonValue) -> String {
    let mut clone = payload.clone();
    if let Some(obj) = clone.as_object_mut() {
        obj.remove("receipt_hash_hex");
    }
    derive_receipt_hash(&clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::ProjectionRow;
    use serde_json::json;

    fn projection_row(event_id: Uuid, decl: Uuid, ts: &str) -> ProjectionRow {
        let payload = json!({
            "declaration_id": decl.to_string(),
            "submitted_at": ts,
            "data": {"a": 1, "b": 2},
        });
        let derived = derive_hash_from_event(&payload);
        ProjectionRow {
            event_id,
            declaration_id: decl,
            event_type: "declaration.submitted.v1".to_string(),
            event_payload: payload,
            receipt_hash_hex: derived,
            ts: ts.to_string(),
        }
    }

    #[test]
    fn authentic_when_all_entries_match() {
        let decl = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let proj = projection_row(eid, decl, "2026-05-12T10:00:00Z");
        let onchain = OnChainEntry {
            event_id: eid.to_string(),
            declaration_id: decl.to_string(),
            receipt_hash_hex: proj.receipt_hash_hex.clone(),
            ts: "2026-05-12T10:00:00Z".to_string(),
            tx_id: "tx-1".to_string(),
        };
        let report = build_report(decl, vec![onchain], vec![proj]);
        assert_eq!(report.result, VerificationResult::Authentic);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].status, EntryStatus::Matched);
    }

    #[test]
    fn tampered_when_hash_mismatches() {
        let decl = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let proj = projection_row(eid, decl, "2026-05-12T10:00:00Z");
        let onchain = OnChainEntry {
            event_id: eid.to_string(),
            declaration_id: decl.to_string(),
            receipt_hash_hex: "ff".repeat(32),
            ts: "2026-05-12T10:00:00Z".to_string(),
            tx_id: "tx-1".to_string(),
        };
        let report = build_report(decl, vec![onchain], vec![proj]);
        assert_eq!(report.result, VerificationResult::Tampered);
        assert_eq!(report.entries[0].status, EntryStatus::Mismatch);
    }

    #[test]
    fn incomplete_when_projection_unanchored() {
        let decl = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let proj = projection_row(eid, decl, "2026-05-12T10:00:00Z");
        let report = build_report(decl, vec![], vec![proj]);
        assert_eq!(report.result, VerificationResult::Incomplete);
        assert_eq!(report.entries[0].status, EntryStatus::MissingOnChain);
    }

    #[test]
    fn tampered_when_onchain_extra() {
        let decl = Uuid::new_v4();
        let phantom_eid = Uuid::new_v4();
        let onchain = OnChainEntry {
            event_id: phantom_eid.to_string(),
            declaration_id: decl.to_string(),
            receipt_hash_hex: "00".repeat(32),
            ts: "2026-05-12T10:00:00Z".to_string(),
            tx_id: "tx-1".to_string(),
        };
        let report = build_report(decl, vec![onchain], vec![]);
        // An on-chain entry the projection has no record of is "tampered"
        // at the conservative end (someone anchored a phantom). Could
        // also be "Incomplete" — chose Tampered because a phantom entry
        // is a stronger signal that something is wrong than a missing
        // anchor.
        assert_eq!(report.result, VerificationResult::Tampered);
        assert_eq!(report.entries[0].status, EntryStatus::MissingProjection);
    }

    #[test]
    fn mixed_authentic_and_incomplete_rolls_up_incomplete() {
        let decl = Uuid::new_v4();
        let eid_a = Uuid::new_v4();
        let eid_b = Uuid::new_v4();
        let proj_a = projection_row(eid_a, decl, "2026-05-12T10:00:00Z");
        let proj_b = projection_row(eid_b, decl, "2026-05-12T11:00:00Z");
        let onchain_a = OnChainEntry {
            event_id: eid_a.to_string(),
            declaration_id: decl.to_string(),
            receipt_hash_hex: proj_a.receipt_hash_hex.clone(),
            ts: "2026-05-12T10:00:00Z".to_string(),
            tx_id: "tx-1".to_string(),
        };
        let report = build_report(decl, vec![onchain_a], vec![proj_a, proj_b]);
        assert_eq!(report.result, VerificationResult::Incomplete);
        assert_eq!(report.entries.len(), 2);
    }
}
