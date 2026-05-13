//! Stage 6 — Pattern detection (R-VER-5).
//!
//! Runs 8 signature queries over the entity-ownership graph projected
//! into Postgres by the writeback subscriber (see migration 0005). Each
//! signature returns a confidence + supporting aggregates; the stage
//! Dempster-combines the per-signature BPAs into one stage BPA.
//!
//! Signatures:
//!   1. Circular ownership          — A owns B, B (transitively) owns A
//!   2. Common-owner pattern        — one person owns > THRESHOLD entities
//!   3. BO of shell company         — entity has no declared BUNEC activity
//!   4. Layered ownership > N       — ownership chain > MAX_DEPTH
//!   5. BO with no prior history    — declarant first-seen < 24h before decl.
//!   6. Sudden ownership change     — > 50pp shift within 30 days
//!   7. Opaque-jurisdiction route   — entity jurisdiction on FATF grey/black
//!   8. Sanctions-adjacent cluster  — owner shares neighbour with sanctions hit
//!
//! Each signature is a method on `PatternDetector`. The stage's `run`
//! invokes each in order; failures from individual signatures are
//! recorded but do not fail the stage. The aggregate BPA is the
//! Dempster combination of every signature's contribution.
//!
//! D17: the signature queries are parameterised; we never interpolate
//! caller data into SQL.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use tracing::warn;

use crate::domain::{
    BasicProbabilityAssignment, DeclarationSnapshot, Stage, StageId, StageOutcome,
    StageOutcomeKind,
};
use crate::metrics::Metrics;

const COMMON_OWNER_THRESHOLD: i64 = 5;
const MAX_OWNERSHIP_DEPTH: i32 = 4;

/// The hard-coded FATF grey + black list (snapshot as of 2026-Q1).
/// A live feed is a follow-up; v1 ships the snapshot. ISO-3166 alpha-2.
pub const FATF_HIGH_RISK: &[&str] = &[
    // Black
    "IR", "KP", "MM",
    // Grey (subset)
    "AF", "AL", "BS", "BB", "BJ", "BG", "BF", "KH", "CD", "GI", "HT",
    "JM", "JO", "ML", "MZ", "NG", "PA", "PH", "SN", "SS", "SY", "TZ",
    "TR", "UG", "AE", "VN", "YE", "ZW",
];

pub struct PatternDetectionStage {
    pool: PgPool,
    metrics: Option<Arc<Metrics>>,
}

impl PatternDetectionStage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, metrics: None }
    }

    pub fn with_metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }
}

#[async_trait]
impl Stage for PatternDetectionStage {
    fn id(&self) -> StageId {
        StageId::PatternDetection
    }

    async fn run(&self, declaration: &DeclarationSnapshot) -> StageOutcome {
        let start = std::time::Instant::now();
        let mut signatures: Vec<SignatureResult> = Vec::with_capacity(8);

        // Each signature is run individually; an Err is recorded but
        // does not abort the stage. The aggregate BPA is built up
        // from the firing signatures.
        let sig_runs: Vec<(&str, BasicProbabilityAssignment, _)> = vec![
            ("circular_ownership", BasicProbabilityAssignment::new(0.10, 0.70, 0.20).unwrap(), self.detect_circular(declaration).await),
            ("common_owner", BasicProbabilityAssignment::new(0.10, 0.50, 0.40).unwrap(), self.detect_common_owner(declaration).await),
            ("bo_of_shell", BasicProbabilityAssignment::new(0.20, 0.40, 0.40).unwrap(), self.detect_bo_of_shell(declaration).await),
            ("layered_ownership", BasicProbabilityAssignment::new(0.20, 0.40, 0.40).unwrap(), self.detect_layered(declaration).await),
            ("no_prior_history", BasicProbabilityAssignment::new(0.30, 0.30, 0.40).unwrap(), self.detect_no_prior_history(declaration).await),
            ("sudden_ownership_change", BasicProbabilityAssignment::new(0.20, 0.50, 0.30).unwrap(), self.detect_sudden_change(declaration).await),
            ("opaque_jurisdiction", BasicProbabilityAssignment::new(0.20, 0.50, 0.30).unwrap(), self.detect_opaque_jurisdiction(declaration).await),
            ("sanctions_adjacent", BasicProbabilityAssignment::new(0.10, 0.60, 0.30).unwrap(), self.detect_sanctions_adjacent(declaration).await),
        ];

        let mut combined = BasicProbabilityAssignment::vacuous();
        let mut combined_risk = BasicProbabilityAssignment::vacuous();

        for (sig_name, fire_bpa, result) in sig_runs {
            let started_sig = std::time::Instant::now();
            let outcome: SignatureResult = match result {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, signature = sig_name, "signature errored");
                    SignatureResult {
                        signature: sig_name.into(),
                        fired: false,
                        aggregates: json!({"error": e}),
                        confidence: 0.0,
                    }
                }
            };
            if outcome.fired {
                // Combine the firing BPA, weighted-down by confidence.
                let weighted = weighted_bpa(fire_bpa, outcome.confidence);
                combined = combined.combine(weighted).unwrap_or_else(|_| combined.combine_yager(weighted));
                let risk_weighted = weighted_bpa(swap_true_false(fire_bpa), outcome.confidence);
                combined_risk = combined_risk.combine(risk_weighted).unwrap_or_else(|_| combined_risk.combine_yager(risk_weighted));
            }
            if let Some(m) = &self.metrics {
                m.pattern_detection_total
                    .with_label_values(&[sig_name, if outcome.fired { "fired" } else { "clean" }])
                    .inc();
                m.pattern_detection_latency_seconds
                    .with_label_values(&[sig_name])
                    .observe(started_sig.elapsed().as_secs_f64());
            }
            signatures.push(outcome);
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let any_fired = signatures.iter().any(|s| s.fired);
        let kind = if !any_fired {
            StageOutcomeKind::Pass
        } else {
            StageOutcomeKind::Fail
        };

        StageOutcome {
            stage_id: StageId::PatternDetection,
            kind,
            authenticity_bpa: combined,
            risk_bpa: combined_risk,
            evidence: json!({
                "signatures": signatures,
                "any_fired": any_fired,
            }),
            duration_ms,
        }
    }
}

/// Allocate `confidence` fraction of the firing BPA's
/// (m_true + m_false) mass; reallocate the rest into ignorance.
fn weighted_bpa(bpa: BasicProbabilityAssignment, confidence: f64) -> BasicProbabilityAssignment {
    let c = confidence.clamp(0.0, 1.0);
    let new_true = bpa.m_true * c;
    let new_false = bpa.m_false * c;
    let new_unc = 1.0 - new_true - new_false;
    BasicProbabilityAssignment::new(new_true, new_false, new_unc).expect("weighted construction valid")
}

fn swap_true_false(bpa: BasicProbabilityAssignment) -> BasicProbabilityAssignment {
    BasicProbabilityAssignment::new(bpa.m_false, bpa.m_true, bpa.m_uncertain).expect("swap valid")
}

#[derive(Debug, Clone, Serialize)]
pub struct SignatureResult {
    pub signature: String,
    pub fired: bool,
    pub aggregates: serde_json::Value,
    pub confidence: f64,
}

// ─── Signature implementations ────────────────────────────────────────

impl PatternDetectionStage {
    async fn detect_circular(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        // Look for any owner who is themselves a beneficial owner of
        // an entity whose owner chain reaches back to `declaration.entity_id`.
        // For v1 we test the direct case (depth-1 cycle).
        let owner_ids: Vec<uuid::Uuid> = declaration
            .beneficial_owners
            .iter()
            .map(|o| o.person_id)
            .collect();
        if owner_ids.is_empty() {
            return Ok(SignatureResult {
                signature: "circular_ownership".into(),
                fired: false,
                aggregates: json!({"reason": "no owners"}),
                confidence: 0.0,
            });
        }
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)::INT8 AS "n!"
            FROM entity_ownership_graph eog
            WHERE eog.entity_id = ANY(
                SELECT entity_id FROM entity_ownership_graph
                WHERE owner_person_id = ANY($1)
            )
            AND eog.owner_person_id = ANY($1)
            AND eog.entity_id = $2
            "#,
            &owner_ids,
            declaration.entity_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(SignatureResult {
            signature: "circular_ownership".into(),
            fired: count > 0,
            aggregates: json!({"matches": count}),
            confidence: if count > 0 { 0.7 } else { 0.0 },
        })
    }

    async fn detect_common_owner(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        let owner_ids: Vec<uuid::Uuid> = declaration
            .beneficial_owners
            .iter()
            .map(|o| o.person_id)
            .collect();
        if owner_ids.is_empty() {
            return Ok(SignatureResult {
                signature: "common_owner".into(),
                fired: false,
                aggregates: json!({"reason": "no owners"}),
                confidence: 0.0,
            });
        }
        let rows = sqlx::query!(
            r#"
            SELECT owner_person_id   AS "owner!: uuid::Uuid",
                   COUNT(DISTINCT entity_id)::INT8 AS "n!"
            FROM entity_ownership_graph
            WHERE owner_person_id = ANY($1)
            GROUP BY owner_person_id
            HAVING COUNT(DISTINCT entity_id) > $2
            "#,
            &owner_ids,
            COMMON_OWNER_THRESHOLD,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let fired = !rows.is_empty();
        let aggregates = json!({
            "matches": rows.iter().map(|r| json!({"owner": r.owner, "entity_count": r.n})).collect::<Vec<_>>(),
            "threshold": COMMON_OWNER_THRESHOLD,
        });
        Ok(SignatureResult {
            signature: "common_owner".into(),
            fired,
            aggregates,
            confidence: if fired { 0.6 } else { 0.0 },
        })
    }

    async fn detect_bo_of_shell(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        let row = sqlx::query!(
            r#"SELECT has_bunec_activity FROM declaration_projection WHERE entity_id = $1 ORDER BY submitted_at DESC LIMIT 1"#,
            declaration.entity_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let fired = matches!(row.as_ref().map(|r| r.has_bunec_activity), Some(false));
        Ok(SignatureResult {
            signature: "bo_of_shell".into(),
            fired,
            aggregates: json!({"has_bunec_activity": row.as_ref().map(|r| r.has_bunec_activity)}),
            confidence: if fired { 0.5 } else { 0.0 },
        })
    }

    async fn detect_layered(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        let max_depth: Option<i32> = sqlx::query_scalar!(
            r#"
            SELECT MAX(depth) AS "depth?"
            FROM ownership_paths
            WHERE root_entity_id = $1
            "#,
            declaration.entity_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let depth = max_depth.unwrap_or(0);
        let fired = depth > MAX_OWNERSHIP_DEPTH;
        Ok(SignatureResult {
            signature: "layered_ownership".into(),
            fired,
            aggregates: json!({"max_depth": depth, "limit": MAX_OWNERSHIP_DEPTH}),
            confidence: if fired { 0.4 } else { 0.0 },
        })
    }

    async fn detect_no_prior_history(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        // Declarant principal first appears in declaration_projection
        // less than 24 hours before the current declaration.
        let earliest: Option<time::OffsetDateTime> = sqlx::query_scalar!(
            r#"
            SELECT MIN(submitted_at) AS "min?"
            FROM declaration_projection
            WHERE declarant_principal = $1
              AND submitted_at < $2
            "#,
            declaration.declarant_principal,
            declaration.submitted_at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let fired = match earliest {
            None => true, // never seen before
            Some(t) => declaration.submitted_at - t < time::Duration::hours(24),
        };
        Ok(SignatureResult {
            signature: "no_prior_history".into(),
            fired,
            aggregates: json!({"earliest_seen": earliest.map(|t| t.to_string())}),
            confidence: if fired { 0.3 } else { 0.0 },
        })
    }

    async fn detect_sudden_change(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        // For each owner in the current declaration, compare against
        // the same owner's basis_points in declarations of the same
        // entity within the last 30 days.
        let owner_ids: Vec<uuid::Uuid> = declaration
            .beneficial_owners
            .iter()
            .map(|o| o.person_id)
            .collect();
        if owner_ids.is_empty() {
            return Ok(SignatureResult {
                signature: "sudden_ownership_change".into(),
                fired: false,
                aggregates: json!({"reason": "no owners"}),
                confidence: 0.0,
            });
        }
        let rows = sqlx::query!(
            r#"
            SELECT eog.owner_person_id    AS "owner!: uuid::Uuid",
                   eog.ownership_basis_points  AS "bp!: i32"
            FROM entity_ownership_graph eog
            WHERE eog.entity_id = $1
              AND eog.owner_person_id = ANY($2)
              AND eog.submitted_at < $3
              AND eog.submitted_at >= $3 - INTERVAL '30 days'
            "#,
            declaration.entity_id,
            &owner_ids,
            declaration.submitted_at,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let mut max_shift = 0_i32;
        for row in &rows {
            if let Some(curr) = declaration
                .beneficial_owners
                .iter()
                .find(|o| o.person_id == row.owner)
            {
                let curr_bp = curr.ownership_basis_points as i32;
                let shift = (curr_bp - row.bp).abs();
                if shift > max_shift {
                    max_shift = shift;
                }
            }
        }
        let fired = max_shift > 5_000; // > 50 percentage points
        Ok(SignatureResult {
            signature: "sudden_ownership_change".into(),
            fired,
            aggregates: json!({"max_shift_bp": max_shift, "threshold_bp": 5_000}),
            confidence: if fired { 0.5 } else { 0.0 },
        })
    }

    async fn detect_opaque_jurisdiction(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        let juris: Option<String> = sqlx::query_scalar!(
            r#"SELECT entity_jurisdiction FROM declaration_projection
               WHERE entity_id = $1 ORDER BY submitted_at DESC LIMIT 1"#,
            declaration.entity_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?
        .flatten();
        let fired = juris
            .as_deref()
            .map(|j| FATF_HIGH_RISK.contains(&j))
            .unwrap_or(false);
        Ok(SignatureResult {
            signature: "opaque_jurisdiction".into(),
            fired,
            aggregates: json!({"jurisdiction": juris}),
            confidence: if fired { 0.5 } else { 0.0 },
        })
    }

    async fn detect_sanctions_adjacent(
        &self,
        declaration: &DeclarationSnapshot,
    ) -> Result<SignatureResult, String> {
        let owner_ids: Vec<uuid::Uuid> = declaration
            .beneficial_owners
            .iter()
            .map(|o| o.person_id)
            .collect();
        if owner_ids.is_empty() {
            return Ok(SignatureResult {
                signature: "sanctions_adjacent".into(),
                fired: false,
                aggregates: json!({"reason": "no owners"}),
                confidence: 0.0,
            });
        }
        // Find owners sharing an entity with someone who has a sanctions hit
        // (joined on canonical name; in v1 we treat any entity_ownership row
        // whose owner appears in sanctions_persons as adjacent).
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)::INT8 AS "n!"
            FROM entity_ownership_graph eog
            WHERE eog.entity_id IN (
                SELECT entity_id FROM entity_ownership_graph
                WHERE owner_person_id = ANY($1)
            )
            AND EXISTS (
                SELECT 1 FROM sanctions_persons sp
                WHERE sp.full_name_canonical = (
                    SELECT canonical_full_name FROM mock_bunec_persons
                    WHERE person_id = eog.owner_person_id
                    LIMIT 1
                )
            )
            "#,
            &owner_ids,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let fired = count > 0;
        Ok(SignatureResult {
            signature: "sanctions_adjacent".into(),
            fired,
            aggregates: json!({"degree1_neighbours_with_sanctions": count}),
            confidence: if fired { 0.5 } else { 0.0 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatf_list_contains_known_grey() {
        assert!(FATF_HIGH_RISK.contains(&"PA"));
        assert!(FATF_HIGH_RISK.contains(&"NG"));
    }

    #[test]
    fn fatf_list_contains_known_black() {
        assert!(FATF_HIGH_RISK.contains(&"IR"));
        assert!(FATF_HIGH_RISK.contains(&"KP"));
        assert!(FATF_HIGH_RISK.contains(&"MM"));
    }

    #[test]
    fn weighted_bpa_at_zero_confidence_is_vacuous() {
        let b = BasicProbabilityAssignment::new(0.1, 0.7, 0.2).unwrap();
        let w = weighted_bpa(b, 0.0);
        assert!((w.m_uncertain - 1.0).abs() < 1e-9);
    }

    #[test]
    fn weighted_bpa_at_one_is_identity() {
        let b = BasicProbabilityAssignment::new(0.1, 0.7, 0.2).unwrap();
        let w = weighted_bpa(b, 1.0);
        assert!((w.m_true - 0.1).abs() < 1e-9);
        assert!((w.m_false - 0.7).abs() < 1e-9);
        assert!((w.m_uncertain - 0.2).abs() < 1e-9);
    }

    #[test]
    fn weighted_bpa_half_confidence() {
        let b = BasicProbabilityAssignment::new(0.2, 0.6, 0.2).unwrap();
        let w = weighted_bpa(b, 0.5);
        // m_true=0.1, m_false=0.3, m_unc=0.6
        assert!((w.m_true - 0.1).abs() < 1e-9);
        assert!((w.m_false - 0.3).abs() < 1e-9);
        assert!((w.m_uncertain - 0.6).abs() < 1e-9);
    }

    #[test]
    fn swap_true_false_inverts() {
        let b = BasicProbabilityAssignment::new(0.1, 0.7, 0.2).unwrap();
        let s = swap_true_false(b);
        assert!((s.m_true - 0.7).abs() < 1e-9);
        assert!((s.m_false - 0.1).abs() < 1e-9);
    }
}
