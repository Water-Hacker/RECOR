//! REST request / response DTOs. Distinct from domain types so the
//! wire shape can evolve independently. Mapping is explicit; no
//! sneaky `#[derive(From)]` shortcuts that would couple them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::application::{DeclarationProjection, SubmitReceipt};
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, EntityId,
    RecordVerificationOutcome, SubmitDeclaration, VerificationLane,
};
use crate::domain::attestation::CryptographicAttestation;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubmitDeclarationRequest {
    pub declaration_id: Option<DeclarationId>,
    pub entity_id: EntityId,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    pub attestation: CryptographicAttestation,
}

impl SubmitDeclarationRequest {
    /// Materialise a `SubmitDeclaration` command from the request body
    /// + the authenticated principal + the request-derived correlation id.
    /// `declarant_principal` comes from auth, not from the request body —
    /// this is the integrity property that prevents principal spoofing.
    pub fn into_command(
        self,
        declarant_principal: String,
        correlation_id: Uuid,
    ) -> SubmitDeclaration {
        SubmitDeclaration {
            declaration_id: self.declaration_id.unwrap_or_default(),
            entity_id: self.entity_id,
            declarant_principal,
            declarant_role: self.declarant_role,
            kind: self.kind,
            effective_from: self.effective_from,
            beneficial_owners: self.beneficial_owners,
            attestation: self.attestation,
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitDeclarationResponse {
    pub declaration_id: DeclarationId,
    pub state: String,
    pub receipt_hash_hex: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: OffsetDateTime,
    pub receipt_url: String,
}

impl SubmitDeclarationResponse {
    pub fn from_receipt(receipt: SubmitReceipt, base_url: &str) -> Self {
        let receipt_url = format!(
            "{base_url}/v1/declarations/{id}",
            id = receipt.declaration_id
        );
        Self {
            declaration_id: receipt.declaration_id,
            state: receipt.state,
            receipt_hash_hex: receipt.receipt_hash_hex,
            submitted_at: receipt.submitted_at,
            receipt_url,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GetDeclarationResponse {
    pub declaration_id: DeclarationId,
    pub entity_id: EntityId,
    pub declarant_principal: String,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    pub state: String,
    pub aggregate_version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: OffsetDateTime,
    pub receipt_hash_hex: String,
    pub correlation_id: Uuid,

    /// Downstream verification engine outcome. Always present:
    /// `not_verified` until the engine writes back, then transitions
    /// to one of (`pending`, `in_verification`, `accepted`, `rejected`).
    pub verification_state: String,
    /// Lane decision the verification engine returned, if it has run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_lane: Option<VerificationLane>,
    /// The verification case_id that produced the current verification
    /// state. Consumers can join this against the verification engine's
    /// case API to retrieve detailed evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_case_id: Option<Uuid>,
    /// Time the verification engine completed the case.
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub verified_at: Option<OffsetDateTime>,

    /// If this declaration replaced an earlier one, the earlier
    /// declaration's id. Consumers can chase backwards through the
    /// supersede chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_declaration_id: Option<DeclarationId>,
    /// If this declaration has been replaced, the successor's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_declaration_id: Option<DeclarationId>,
    /// Time this declaration was superseded.
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub superseded_at: Option<OffsetDateTime>,
}

impl From<DeclarationProjection> for GetDeclarationResponse {
    fn from(p: DeclarationProjection) -> Self {
        Self {
            declaration_id: p.declaration_id,
            entity_id: p.entity_id,
            declarant_principal: p.declarant_principal,
            declarant_role: p.declarant_role,
            kind: p.kind,
            effective_from: p.effective_from,
            beneficial_owners: p.beneficial_owners,
            state: p.state.as_str().to_string(),
            aggregate_version: p.version,
            submitted_at: p.submitted_at,
            receipt_hash_hex: p.receipt_hash_hex,
            correlation_id: p.correlation_id,
            verification_state: p.verification_state,
            verification_lane: p.verification_lane,
            verification_case_id: p.verification_case_id,
            verified_at: p.verified_at,
            supersedes_declaration_id: p.supersedes_declaration_id,
            superseded_by_declaration_id: p.superseded_by_declaration_id,
            superseded_at: p.superseded_at,
        }
    }
}

/// Receipt for a successful supersede. The new declaration's id is
/// the consumer's handle going forward; the old declaration's id is
/// echoed back so callers can confirm the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersedeDeclarationResponse {
    pub new_declaration_id: DeclarationId,
    pub superseded_declaration_id: DeclarationId,
    pub state: String,
    pub receipt_hash_hex: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    pub submitted_at: OffsetDateTime,
    pub receipt_url: String,
}

impl SupersedeDeclarationResponse {
    pub fn from_receipt(
        receipt: crate::application::SupersedeReceipt,
        base_url: &str,
    ) -> Self {
        Self {
            new_declaration_id: receipt.new_declaration_id,
            superseded_declaration_id: receipt.superseded_declaration_id,
            state: receipt.state,
            receipt_hash_hex: receipt.receipt_hash_hex,
            submitted_at: receipt.submitted_at,
            receipt_url: format!(
                "{base_url}/v1/declarations/{id}",
                id = receipt.new_declaration_id
            ),
        }
    }
}

/// Inbound envelope on POST /v1/internal/verification-outcomes.
///
/// Field names + types MUST match the verification engine's outbox
/// payload exactly. See
/// services/verification-engine/src/infrastructure/postgres.rs writeback
/// payload construction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerificationOutcomeRequest {
    pub case_id: Uuid,
    pub declaration_id: DeclarationId,
    pub lane: VerificationLane,
    pub fused_authenticity_belief: f64,
    pub fused_authenticity_plausibility: f64,
    pub fused_risk_belief: f64,
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: OffsetDateTime,
}

impl VerificationOutcomeRequest {
    pub fn into_command(self) -> RecordVerificationOutcome {
        RecordVerificationOutcome {
            declaration_id: self.declaration_id,
            verification_case_id: self.case_id,
            lane: self.lane,
            fused_authenticity_belief: self.fused_authenticity_belief,
            fused_authenticity_plausibility: self.fused_authenticity_plausibility,
            fused_risk_belief: self.fused_risk_belief,
            completed_at: self.completed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationOutcomeResponse {
    pub declaration_id: DeclarationId,
    pub verification_case_id: Uuid,
    pub lane: VerificationLane,
    pub recorded_new_event: bool,
}
