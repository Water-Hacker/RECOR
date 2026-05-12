//! REST request / response DTOs.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::application::{MergeReceipt, PersonProjection, RegisterReceipt};
use crate::domain::value_object::PersonAttributes;
use crate::domain::{PersonId, RegisterPerson};

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RegisterPersonRequest {
    /// Optional client-supplied id (UUIDv7). Omitted → service mints one.
    pub person_id: Option<PersonId>,
    pub attributes: PersonAttributes,
}

impl RegisterPersonRequest {
    pub fn into_command(
        self,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> RegisterPerson {
        RegisterPerson {
            person_id: self.person_id.unwrap_or_default(),
            attributes: self.attributes,
            actor_principal,
            registered_at: OffsetDateTime::now_utc(),
            correlation_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterPersonResponse {
    pub person_id: PersonId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-12T10:00:00.000Z")]
    pub registered_at: OffsetDateTime,
    /// Self-link to the persisted person record.
    pub receipt_url: String,
}

impl RegisterPersonResponse {
    pub fn from_receipt(receipt: RegisterReceipt, base_url: &str) -> Self {
        let receipt_url = format!(
            "{base_url}/v1/persons/{id}",
            id = receipt.person_id
        );
        Self {
            person_id: receipt.person_id,
            registered_at: receipt.registered_at,
            receipt_url,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GetPersonResponse {
    pub person_id: PersonId,
    pub attributes: PersonAttributes,
    pub aggregate_version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-12T10:00:00.000Z")]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-12T10:00:00.000Z")]
    pub updated_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<PersonId>,
}

impl From<PersonProjection> for GetPersonResponse {
    fn from(p: PersonProjection) -> Self {
        Self {
            person_id: p.person_id,
            attributes: p.attributes,
            aggregate_version: p.aggregate_version,
            created_at: p.created_at,
            updated_at: p.updated_at,
            merged_into: p.merged_into,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchPersonsResponse {
    pub items: Vec<GetPersonResponse>,
    pub count: usize,
}

impl SearchPersonsResponse {
    pub fn from_projections(rows: Vec<PersonProjection>) -> Self {
        let items: Vec<GetPersonResponse> = rows.into_iter().map(Into::into).collect();
        let count = items.len();
        Self { items, count }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MergePersonsResponse {
    pub from_person_id: PersonId,
    pub into_person_id: PersonId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime, example = "2026-05-12T10:00:00.000Z")]
    pub merged_at: OffsetDateTime,
}

impl From<MergeReceipt> for MergePersonsResponse {
    fn from(r: MergeReceipt) -> Self {
        Self {
            from_person_id: r.from_person_id,
            into_person_id: r.into_person_id,
            merged_at: r.merged_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthzResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReadyzResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}
