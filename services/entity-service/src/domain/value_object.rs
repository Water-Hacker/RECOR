//! Domain value objects — newtype-wrapped primitives that carry domain
//! meaning the underlying primitive does not.
//!
//! `ToSchema` lives on every type that crosses the public wire so the
//! OpenAPI spec (DOC-1) can describe them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Stable identifier for a legal entity. UUIDv7 — time-sortable, so the
/// natural ordering matches registration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(
    value_type = String,
    format = "uuid",
    example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d"
)]
pub struct EntityId(pub Uuid);

impl EntityId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// ISO-3166-1 alpha-2 country code. Stored upper-case CHAR(2). Validated
/// on construction; the projection's CHECK constraint mirrors the
/// validation so a bypass at the API layer is still refused at the DB.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, example = "CM")]
pub struct Jurisdiction(String);

impl Jurisdiction {
    pub fn try_from_str(raw: &str) -> Result<Self, ValueObjectError> {
        let trimmed = raw.trim();
        if trimmed.len() != 2 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ValueObjectError::InvalidJurisdiction(trimmed.to_string()));
        }
        Ok(Self(trimmed.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Jurisdiction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Canonical name of the entity. Statutorily public under Cameroon's
/// transparency framework. Trimmed; 1..=512 characters; rejects empty
/// strings and over-long values at construction time so the aggregate
/// can be trusted thereafter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, example = "Cameroon Mining Holdings SARL")]
pub struct CanonicalName(String);

impl CanonicalName {
    pub fn try_from_str(raw: &str) -> Result<Self, ValueObjectError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ValueObjectError::EmptyCanonicalName);
        }
        if trimmed.chars().count() > 512 {
            return Err(ValueObjectError::CanonicalNameTooLong(trimmed.chars().count()));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CanonicalName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Registration number in the jurisdiction's business register. For
/// Cameroonian entities this eventually matches BUNEC's surface
/// (deferred to R-VER-1). 1..=128 characters; trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, example = "RC/DLA/2024/B/12345")]
pub struct RegistrationNumber(String);

impl RegistrationNumber {
    pub fn try_from_str(raw: &str) -> Result<Self, ValueObjectError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ValueObjectError::EmptyRegistrationNumber);
        }
        if trimmed.chars().count() > 128 {
            return Err(ValueObjectError::RegistrationNumberTooLong(
                trimmed.chars().count(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RegistrationNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Type / legal form of the entity. Closed set for the OHADA-region
/// standard forms (SA, SARL, Partnership, Trust), with an `Other(String)`
/// escape hatch for forms that are not in the closed set — necessary
/// because the registry holds non-Cameroonian entities whose legal
/// forms are not standard OHADA forms.
///
/// Wire format: snake_case enum tag. `Other` carries its free-form
/// label as an associated string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "label", rename_all = "snake_case")]
pub enum EntityType {
    /// Société Anonyme.
    Sa,
    /// Société à Responsabilité Limitée.
    Sarl,
    /// General partnership / SNC (Société en Nom Collectif) / similar.
    Partnership,
    /// Trust / fiducie.
    Trust,
    /// Free-form label for legal forms not in the closed set.
    Other(String),
}

impl EntityType {
    pub fn try_from_wire(kind: &str, label: Option<&str>) -> Result<Self, ValueObjectError> {
        match (kind, label) {
            ("sa", _) => Ok(Self::Sa),
            ("sarl", _) => Ok(Self::Sarl),
            ("partnership", _) => Ok(Self::Partnership),
            ("trust", _) => Ok(Self::Trust),
            ("other", Some(l)) if !l.trim().is_empty() => {
                let trimmed = l.trim();
                if trimmed.chars().count() > 64 {
                    return Err(ValueObjectError::EntityTypeLabelTooLong(trimmed.chars().count()));
                }
                Ok(Self::Other(trimmed.to_string()))
            }
            ("other", _) => Err(ValueObjectError::OtherEntityTypeMissingLabel),
            (k, _) => Err(ValueObjectError::UnknownEntityType(k.to_string())),
        }
    }

    /// Stable string representation for projection storage. The
    /// `Other(...)` variant is serialised as `other:<label>` so the
    /// round-trip is exact, and the closed-set variants are bare
    /// snake_case so the column is searchable without JSON parsing.
    pub fn as_storage_string(&self) -> String {
        match self {
            Self::Sa => "sa".to_string(),
            Self::Sarl => "sarl".to_string(),
            Self::Partnership => "partnership".to_string(),
            Self::Trust => "trust".to_string(),
            Self::Other(label) => format!("other:{label}"),
        }
    }

    /// Reverse of `as_storage_string`.
    pub fn from_storage_string(s: &str) -> Result<Self, ValueObjectError> {
        if let Some(label) = s.strip_prefix("other:") {
            return Self::try_from_wire("other", Some(label));
        }
        Self::try_from_wire(s, None)
    }
}

/// The set of fields updatable in-place on an existing entity.
///
/// Used by `UpdateEntity` commands and stored in `EntityUpdatedV1`
/// events (both as the `before` snapshot — what the aggregate held —
/// and the `after` snapshot — what the actor is replacing them with).
/// Fields NOT in this set cannot be updated in place:
///   - `id` (identity-immutable)
///   - `jurisdiction` and `registration_number_in_jurisdiction` (the
///     identity tuple; changing them would create a new entity)
///   - `founded_at` (historical fact; rectification of an incorrect
///     founded_at is a separate procedural flow not in v1 scope)
///   - `dissolved_at` (use the dedicated dissolve endpoint)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdatableFields {
    pub canonical_name: CanonicalName,
    pub entity_type: EntityType,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValueObjectError {
    #[error("invalid jurisdiction code `{0}`; expected ISO-3166-1 alpha-2 (two ASCII letters)")]
    InvalidJurisdiction(String),
    #[error("canonical name is empty after trimming")]
    EmptyCanonicalName,
    #[error("canonical name is {0} characters; maximum 512")]
    CanonicalNameTooLong(usize),
    #[error("registration number is empty after trimming")]
    EmptyRegistrationNumber,
    #[error("registration number is {0} characters; maximum 128")]
    RegistrationNumberTooLong(usize),
    #[error("entity type `other` requires a non-empty label")]
    OtherEntityTypeMissingLabel,
    #[error("entity type label is {0} characters; maximum 64")]
    EntityTypeLabelTooLong(usize),
    #[error("unknown entity type `{0}`; expected one of: sa, sarl, partnership, trust, other")]
    UnknownEntityType(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jurisdiction_accepts_two_letter_code() {
        let j = Jurisdiction::try_from_str("cm").unwrap();
        assert_eq!(j.as_str(), "CM");
    }

    #[test]
    fn jurisdiction_rejects_wrong_length() {
        assert!(Jurisdiction::try_from_str("CMR").is_err());
        assert!(Jurisdiction::try_from_str("C").is_err());
        assert!(Jurisdiction::try_from_str("").is_err());
    }

    #[test]
    fn jurisdiction_rejects_non_ascii() {
        assert!(Jurisdiction::try_from_str("12").is_err());
        assert!(Jurisdiction::try_from_str("é2").is_err());
    }

    #[test]
    fn canonical_name_rejects_empty() {
        assert!(CanonicalName::try_from_str("").is_err());
        assert!(CanonicalName::try_from_str("    ").is_err());
    }

    #[test]
    fn canonical_name_trims() {
        let n = CanonicalName::try_from_str("  ACME  ").unwrap();
        assert_eq!(n.as_str(), "ACME");
    }

    #[test]
    fn entity_type_round_trips_through_storage() {
        for original in [
            EntityType::Sa,
            EntityType::Sarl,
            EntityType::Partnership,
            EntityType::Trust,
            EntityType::Other("cooperative".into()),
        ] {
            let storage = original.as_storage_string();
            let parsed = EntityType::from_storage_string(&storage).unwrap();
            assert_eq!(parsed, original);
        }
    }

    #[test]
    fn entity_type_other_requires_label() {
        assert!(EntityType::try_from_wire("other", None).is_err());
        assert!(EntityType::try_from_wire("other", Some("")).is_err());
        assert!(EntityType::try_from_wire("other", Some("ok")).is_ok());
    }

    #[test]
    fn entity_id_new_is_time_sortable() {
        let a = EntityId::new();
        let b = EntityId::new();
        assert!(b.0 > a.0, "uuidv7 must produce sortable identifiers");
    }
}
