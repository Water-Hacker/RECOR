//! REST request / response DTOs. Distinct from domain types so the
//! wire shape can evolve independently. Mapping is explicit; no
//! sneaky `#[derive(From)]` shortcuts that would couple them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::application::{DeclarationProjection, SubmitReceipt};
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, EntityId, SubmitDeclaration,
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
        }
    }
}
