//! Wire DTOs for the `/v1/arrangements` REST surface (TODO-002-domain).
//!
//! Mirrors the shape of the domain commands / events; DTOs are kept
//! decoupled from the canonical domain types so the public contract can
//! evolve independently. Each DTO carries a `ToSchema` derive so the
//! OpenAPI spec (DOC-1) can describe it.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::application::{ArrangementProjection, RegisterArrangementReceipt};
use crate::domain::{
    ArrangementDomainError, ArrangementId, ArrangementKind, ArrangementUpdatableFields,
    ClassBeneficiarySpec, ControlExerciseRef, DissolveArrangement, GoverningLawJurisdiction,
    NamedBeneficiaryRef, ProtectorRef, RegisterArrangement, SettlorRef, TrusteeRef,
    UpdateArrangement,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrangementKindDto {
    ExpressTrust,
    Fiducy,
    Waqf,
    Similar,
}

impl ArrangementKindDto {
    pub fn into_domain(self) -> ArrangementKind {
        match self {
            Self::ExpressTrust => ArrangementKind::ExpressTrust,
            Self::Fiducy => ArrangementKind::Fiducy,
            Self::Waqf => ArrangementKind::Waqf,
            Self::Similar => ArrangementKind::Similar,
        }
    }

    pub fn from_domain(k: ArrangementKind) -> Self {
        match k {
            ArrangementKind::ExpressTrust => Self::ExpressTrust,
            ArrangementKind::Fiducy => Self::Fiducy,
            ArrangementKind::Waqf => Self::Waqf,
            ArrangementKind::Similar => Self::Similar,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SettlorRefDto {
    #[schema(value_type = String, format = "uuid")]
    pub person_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

impl From<SettlorRefDto> for SettlorRef {
    fn from(d: SettlorRefDto) -> Self {
        Self {
            person_id: d.person_id,
            role_metadata: d.role_metadata,
        }
    }
}

impl From<SettlorRef> for SettlorRefDto {
    fn from(v: SettlorRef) -> Self {
        Self {
            person_id: v.person_id,
            role_metadata: v.role_metadata,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrusteeRefDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub person_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub entity_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiduciary_registration_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

impl From<TrusteeRefDto> for TrusteeRef {
    fn from(d: TrusteeRefDto) -> Self {
        Self {
            person_id: d.person_id,
            entity_id: d.entity_id,
            fiduciary_registration_id: d.fiduciary_registration_id,
            role_metadata: d.role_metadata,
        }
    }
}

impl From<TrusteeRef> for TrusteeRefDto {
    fn from(v: TrusteeRef) -> Self {
        Self {
            person_id: v.person_id,
            entity_id: v.entity_id,
            fiduciary_registration_id: v.fiduciary_registration_id,
            role_metadata: v.role_metadata,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProtectorRefDto {
    #[schema(value_type = String, format = "uuid")]
    pub person_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

impl From<ProtectorRefDto> for ProtectorRef {
    fn from(d: ProtectorRefDto) -> Self {
        Self {
            person_id: d.person_id,
            role_metadata: d.role_metadata,
        }
    }
}

impl From<ProtectorRef> for ProtectorRefDto {
    fn from(v: ProtectorRef) -> Self {
        Self {
            person_id: v.person_id,
            role_metadata: v.role_metadata,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NamedBeneficiaryRefDto {
    #[schema(value_type = String, format = "uuid")]
    pub person_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

impl From<NamedBeneficiaryRefDto> for NamedBeneficiaryRef {
    fn from(d: NamedBeneficiaryRefDto) -> Self {
        Self {
            person_id: d.person_id,
            role_metadata: d.role_metadata,
        }
    }
}

impl From<NamedBeneficiaryRef> for NamedBeneficiaryRefDto {
    fn from(v: NamedBeneficiaryRef) -> Self {
        Self {
            person_id: v.person_id,
            role_metadata: v.role_metadata,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClassBeneficiarySpecDto {
    pub class: String,
    #[schema(value_type = Object)]
    pub criteria: JsonValue,
}

impl From<ClassBeneficiarySpecDto> for ClassBeneficiarySpec {
    fn from(d: ClassBeneficiarySpecDto) -> Self {
        Self {
            class: d.class,
            criteria: d.criteria,
        }
    }
}

impl From<ClassBeneficiarySpec> for ClassBeneficiarySpecDto {
    fn from(v: ClassBeneficiarySpec) -> Self {
        Self {
            class: v.class,
            criteria: v.criteria,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ControlExerciseRefDto {
    #[schema(value_type = String, format = "uuid")]
    pub person_id: Uuid,
    pub control_basis: String,
}

impl From<ControlExerciseRefDto> for ControlExerciseRef {
    fn from(d: ControlExerciseRefDto) -> Self {
        Self {
            person_id: d.person_id,
            control_basis: d.control_basis,
        }
    }
}

impl From<ControlExerciseRef> for ControlExerciseRefDto {
    fn from(v: ControlExerciseRef) -> Self {
        Self {
            person_id: v.person_id,
            control_basis: v.control_basis,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArrangementFieldsDto {
    pub settlor_refs: Vec<SettlorRefDto>,
    pub trustee_refs: Vec<TrusteeRefDto>,
    #[serde(default)]
    pub protector_refs: Vec<ProtectorRefDto>,
    #[serde(default)]
    pub named_beneficiary_refs: Vec<NamedBeneficiaryRefDto>,
    #[serde(default)]
    pub class_beneficiary_specs: Vec<ClassBeneficiarySpecDto>,
    #[serde(default)]
    pub control_exercise_refs: Vec<ControlExerciseRefDto>,
}

impl ArrangementFieldsDto {
    pub fn into_domain(self) -> ArrangementUpdatableFields {
        ArrangementUpdatableFields {
            settlor_refs: self.settlor_refs.into_iter().map(Into::into).collect(),
            trustee_refs: self.trustee_refs.into_iter().map(Into::into).collect(),
            protector_refs: self.protector_refs.into_iter().map(Into::into).collect(),
            named_beneficiary_refs: self
                .named_beneficiary_refs
                .into_iter()
                .map(Into::into)
                .collect(),
            class_beneficiary_specs: self
                .class_beneficiary_specs
                .into_iter()
                .map(Into::into)
                .collect(),
            control_exercise_refs: self
                .control_exercise_refs
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }

    pub fn from_domain(v: ArrangementUpdatableFields) -> Self {
        Self {
            settlor_refs: v.settlor_refs.into_iter().map(Into::into).collect(),
            trustee_refs: v.trustee_refs.into_iter().map(Into::into).collect(),
            protector_refs: v.protector_refs.into_iter().map(Into::into).collect(),
            named_beneficiary_refs: v
                .named_beneficiary_refs
                .into_iter()
                .map(Into::into)
                .collect(),
            class_beneficiary_specs: v
                .class_beneficiary_specs
                .into_iter()
                .map(Into::into)
                .collect(),
            control_exercise_refs: v
                .control_exercise_refs
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterArrangementRequest {
    pub arrangement_kind: ArrangementKindDto,
    /// ISO-3166-1 alpha-2 governing-law jurisdiction (e.g. `"CM"`).
    #[schema(value_type = String, pattern = "^[A-Za-z]{2}$")]
    pub governing_law_jurisdiction: String,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2024-06-01")]
    pub constitution_date: time::Date,
    pub fields: ArrangementFieldsDto,
}

impl RegisterArrangementRequest {
    pub fn into_command(
        self,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> Result<RegisterArrangement, ArrangementDomainError> {
        let jurisdiction = GoverningLawJurisdiction::try_from_str(
            &self.governing_law_jurisdiction,
        )
        .map_err(|e| ArrangementDomainError::ValueObject(
            crate::domain::ArrangementValueObjectError::UnknownArrangementKind(e.to_string()),
        ))?;
        Ok(RegisterArrangement {
            arrangement_id: ArrangementId::new(),
            arrangement_kind: self.arrangement_kind.into_domain(),
            governing_law_jurisdiction: jurisdiction,
            constitution_date: self.constitution_date,
            fields: self.fields.into_domain(),
            registered_by_principal: actor_principal,
            registered_at: OffsetDateTime::now_utc(),
            correlation_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterArrangementResponse {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub registered_at: OffsetDateTime,
    pub self_url: String,
}

impl RegisterArrangementResponse {
    pub fn from_receipt(receipt: RegisterArrangementReceipt, base_url: &str) -> Self {
        let self_url = format!("{}/v1/arrangements/{}", base_url, receipt.arrangement_id);
        Self {
            arrangement_id: receipt.arrangement_id,
            registered_at: receipt.registered_at,
            self_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateArrangementRequest {
    pub fields: ArrangementFieldsDto,
}

impl UpdateArrangementRequest {
    pub fn into_command(
        self,
        arrangement_id: ArrangementId,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> UpdateArrangement {
        UpdateArrangement {
            arrangement_id,
            after: self.fields.into_domain(),
            updated_by_principal: actor_principal,
            updated_at: OffsetDateTime::now_utc(),
            correlation_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateArrangementResponse {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DissolveArrangementRequest {
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date, example = "2026-04-01")]
    pub dissolution_date: time::Date,
}

impl DissolveArrangementRequest {
    pub fn into_command(
        self,
        arrangement_id: ArrangementId,
        actor_principal: String,
        correlation_id: Uuid,
    ) -> DissolveArrangement {
        DissolveArrangement {
            arrangement_id,
            dissolution_date: self.dissolution_date,
            dissolved_by_principal: actor_principal,
            recorded_at: OffsetDateTime::now_utc(),
            correlation_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DissolveArrangementResponse {
    pub arrangement_id: ArrangementId,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub dissolution_date: time::Date,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub retention_until: time::Date,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetArrangementResponse {
    pub arrangement_id: ArrangementId,
    pub arrangement_kind: ArrangementKindDto,
    pub governing_law_jurisdiction: GoverningLawJurisdiction,
    #[serde(with = "crate::domain::serde_helpers::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub constitution_date: time::Date,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date)]
    pub dissolution_date: Option<time::Date>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::domain::serde_helpers::iso_date_opt"
    )]
    #[schema(value_type = Option<String>, format = Date)]
    pub retention_until: Option<time::Date>,
    pub fields: ArrangementFieldsDto,
    pub version: u64,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "crate::domain::serde_helpers::iso_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

impl From<ArrangementProjection> for GetArrangementResponse {
    fn from(p: ArrangementProjection) -> Self {
        Self {
            arrangement_id: p.arrangement_id,
            arrangement_kind: ArrangementKindDto::from_domain(p.arrangement_kind),
            governing_law_jurisdiction: p.governing_law_jurisdiction,
            constitution_date: p.constitution_date,
            dissolution_date: p.dissolution_date,
            retention_until: p.retention_until,
            fields: ArrangementFieldsDto::from_domain(p.fields),
            version: p.version,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}
