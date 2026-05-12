//! REST request / response DTOs. Distinct from domain types so the
//! wire shape can evolve independently. Mapping is explicit; no
//! sneaky `#[derive(From)]` shortcuts that would couple them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::application::{DeclarationProjection, SubmitReceipt};
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, EntityId,
    RecordVerificationOutcome, SubmitDeclaration, VerificationLane,
};
use crate::domain::attestation::CryptographicAttestation;

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SubmitDeclarationRequest {
    /// Optional client-supplied declaration id. When omitted, the
    /// service mints a `UUIDv7` (time-sortable). Useful for clients that
    /// want to know the id before the round trip completes.
    pub declaration_id: Option<DeclarationId>,
    pub entity_id: EntityId,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    /// Effective date of the declaration, ISO-8601 `YYYY-MM-DD`.
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2026-05-01")]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmitDeclarationResponse {
    pub declaration_id: DeclarationId,
    /// Lifecycle state of the declaration immediately after the write —
    /// almost always `submitted`. See `DeclarationState` for the full
    /// enumeration.
    #[schema(example = "submitted")]
    pub state: String,
    /// BLAKE3-256 hash over the canonical receipt bytes, hex-encoded.
    #[schema(example = "5b4f24c63bda0b6a3c9e7a6f6e2c4d8a9b3c2d1e4f5a6b7c8d9e0f1a2b3c4d5e")]
    pub receipt_hash_hex: String,
    /// Submission timestamp in ISO-8601 (server clock).
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-11T22:39:52.447Z")]
    pub submitted_at: OffsetDateTime,
    /// Self-link to the persisted declaration record.
    #[schema(example = "https://recor.cm/v1/declarations/0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d")]
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

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GetDeclarationResponse {
    pub declaration_id: DeclarationId,
    pub entity_id: EntityId,
    /// Principal subject of the declarant who submitted this record.
    /// Sourced from the authenticated principal at submit-time; clients
    /// MUST NOT trust the value in the request body.
    pub declarant_principal: String,
    pub declarant_role: DeclarantRole,
    pub kind: DeclarationKind,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2026-05-01")]
    pub effective_from: time::Date,
    pub beneficial_owners: Vec<BeneficialOwnerClaim>,
    /// Lifecycle state. One of `draft`, `submitted`, `in_verification`,
    /// `accepted`, `rejected`, `superseded`.
    pub state: String,
    /// Event-sourced aggregate version (monotonic per declaration).
    pub aggregate_version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-11T22:39:52.447Z")]
    pub submitted_at: OffsetDateTime,
    pub receipt_hash_hex: String,
    /// UUID that links the submission to the downstream verification case.
    #[schema(value_type = String, format = "uuid")]
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
    #[schema(value_type = Option<String>, format = "uuid")]
    pub verification_case_id: Option<Uuid>,
    /// Time the verification engine completed the case.
    #[serde(
        with = "crate::domain::serde_helpers::iso_datetime_option",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[schema(value_type = Option<String>, format = DateTime)]
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
    #[schema(value_type = Option<String>, format = DateTime)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SupersedeDeclarationResponse {
    /// Identifier of the freshly-minted successor declaration.
    pub new_declaration_id: DeclarationId,
    /// Identifier of the declaration that was just superseded.
    pub superseded_declaration_id: DeclarationId,
    /// Lifecycle state of the new declaration — always `submitted`.
    pub state: String,
    /// BLAKE3-256 hash over the canonical receipt bytes, hex-encoded.
    pub receipt_hash_hex: String,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-11T22:39:52.447Z")]
    pub submitted_at: OffsetDateTime,
    /// Self-link to the new declaration record.
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
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct VerificationOutcomeRequest {
    #[schema(value_type = String, format = "uuid")]
    pub case_id: Uuid,
    pub declaration_id: DeclarationId,
    pub lane: VerificationLane,
    /// Dempster-Shafer belief in payload authenticity, in [0.0, 1.0].
    #[schema(minimum = 0.0, maximum = 1.0)]
    pub fused_authenticity_belief: f64,
    /// Dempster-Shafer plausibility of payload authenticity, in [0.0, 1.0].
    #[schema(minimum = 0.0, maximum = 1.0)]
    pub fused_authenticity_plausibility: f64,
    /// Dempster-Shafer belief that this declaration carries risk, in [0.0, 1.0].
    #[schema(minimum = 0.0, maximum = 1.0)]
    pub fused_risk_belief: f64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationOutcomeResponse {
    pub declaration_id: DeclarationId,
    #[schema(value_type = String, format = "uuid")]
    pub verification_case_id: Uuid,
    pub lane: VerificationLane,
    /// `true` when the outcome produced a new event; `false` on a
    /// no-op replay (the verification engine relay is at-least-once,
    /// so this distinguishes new outcomes from retries).
    pub recorded_new_event: bool,
}

// ─── Cross-cutting response envelopes ──────────────────────────────────
//
// Every 4xx/5xx response from this service emits the same JSON shape:
//
//   { "error": { "kind": "<machine>", "message": "<human>" } }
//
// Declared once here as a single schema so the OpenAPI spec references
// it from every error response. See `services/declaration/src/error.rs`
// for the producer (ServiceError::into_response).

/// Standard error response body. Every non-2xx response is shaped like
/// `{ "error": { "kind": "<machine>", "message": "<human>" } }`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// Inner body of an `ErrorEnvelope`. `kind` is a stable machine-friendly
/// classifier — clients SHOULD switch on it. `message` is a human
/// description for logs; clients SHOULD NOT parse it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// Machine-friendly classifier (e.g. `bad_request`, `not_found`,
    /// `conflict`, `forbidden`, `authentication_required`,
    /// `idempotency_conflict`, `bad_attestation`, `internal`,
    /// `optimistic_concurrency_conflict`, `admin_disabled`, `not_admin`,
    /// `missing_signature`, `bad_signature`, `writeback_disabled`,
    /// `dlq_row_not_found`, `malformed_envelope`, `malformed_payload`).
    pub kind: String,
    pub message: String,
}

/// Healthz payload — always `{"status":"ok"}`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthzResponse {
    #[schema(example = "ok")]
    pub status: String,
}

/// Readyz payload. `status` is one of `ready` / `not_ready`. When the
/// service is not ready, `reason` describes why (e.g.
/// `database_unreachable`).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReadyzResponse {
    #[schema(example = "ready")]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
