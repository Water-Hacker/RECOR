//! Domain value objects for the Person service.
//!
//! Newtype wrappers carrying domain meaning beyond the underlying
//! primitive. `ToSchema` lives on every type that crosses the public
//! wire so the OpenAPI spec (DOC-1) describes them.
//!
//! ## PII classification — see `docs/compliance/data-classification.md`
//!
//! - `PersonId`: **Public** in isolation, but pairing it with
//!   `CanonicalFullName` (or any other column on the `persons` table)
//!   makes the combination PII-laden. The redaction layer
//!   (OPS-2) keys on `person_id` field names regardless.
//! - `CanonicalFullName`, `Nationality`: **PII**.
//! - `IdDocument`: **Sensitive-PII** — government identity-document
//!   numbers are categorical Sensitive-PII.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

/// Stable identifier for a natural person. UUIDv7 — time-sortable, so
/// natural ordering matches registration order. Same shape as the
/// `PersonId` referenced from the Declaration service's beneficial-owner
/// claims (intentional — the two services share the type byte-for-byte
/// on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String, format = "uuid", example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d")]
pub struct PersonId(pub Uuid);

impl PersonId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for PersonId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PersonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// ISO 3166-1 alpha-2 country code. Two ASCII uppercase letters; the
/// constructor refuses anything else. Stored as a 2-char `TEXT CHAR(2)`
/// in the `persons` projection (see `migrations/0001_init.sql`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, example = "CM", pattern = "^[A-Z]{2}$")]
pub struct Nationality(String);

impl Nationality {
    /// Construct from a two-letter country code. Validates length AND
    /// alphabet (ASCII uppercase). Invalid input → `ValueObjectError`.
    pub fn try_new(raw: impl Into<String>) -> Result<Self, ValueObjectError> {
        let s = raw.into();
        if s.len() != 2 {
            return Err(ValueObjectError::InvalidNationality {
                given: s,
                reason: "must be exactly 2 characters (ISO 3166-1 alpha-2)",
            });
        }
        if !s.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(ValueObjectError::InvalidNationality {
                given: s,
                reason: "must be two ASCII uppercase letters",
            });
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Nationality {
    type Error = ValueObjectError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<Nationality> for String {
    fn from(value: Nationality) -> Self {
        value.0
    }
}

impl std::fmt::Display for Nationality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Canonical full name as stored in the registry.
///
/// The constructor trims surrounding whitespace and refuses the empty
/// string AND strings longer than 512 characters (the same bound the
/// `declarant_principal` column carries on the Declaration service —
/// keep them aligned so we don't end up with one service that accepts
/// a 1024-char name and another that doesn't).
///
/// Higher-fidelity canonicalisation (Unicode NFC + locale-aware case
/// folding) is deferred to a future ticket; v1 keeps the input
/// verbatim after the length + trim normalisation so the round-trip
/// is byte-exact for ASCII names and predictable for diacritics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, example = "Ngono Marie-Claire")]
pub struct CanonicalFullName(String);

impl CanonicalFullName {
    pub const MAX_LEN: usize = 512;

    pub fn try_new(raw: impl Into<String>) -> Result<Self, ValueObjectError> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(ValueObjectError::EmptyName);
        }
        if trimmed.chars().count() > Self::MAX_LEN {
            return Err(ValueObjectError::NameTooLong {
                length: trimmed.chars().count(),
                max: Self::MAX_LEN,
            });
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CanonicalFullName {
    type Error = ValueObjectError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<CanonicalFullName> for String {
    fn from(value: CanonicalFullName) -> Self {
        value.0
    }
}

impl std::fmt::Display for CanonicalFullName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Primary identity-document descriptor. Sensitive-PII per the data
/// classification matrix — field-level encryption REQUIRED at rest
/// once `R-ENC-FIELD-LEVEL` ships. For the v1 skeleton the value lives
/// in plain JSONB; the migration carries a `COMMENT` flag to keep the
/// fact obvious to anyone looking at the schema.
///
/// `expiry` is optional because a few national ID systems don't issue
/// expiring documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IdDocument {
    /// Issuing authority (e.g. `"CM:DGSN"` for Cameroonian Sûreté
    /// Nationale, `"FR:ANTS"` for the French Agence Nationale des
    /// Titres Sécurisés). Free-form for v1; an enum lands once the
    /// authority list stabilises.
    #[schema(example = "CM:DGSN")]
    pub issuer: String,
    /// Document type discriminator.
    pub doc_type: IdDocumentType,
    /// Document number / serial. Government-issued identity numbers
    /// are categorical Sensitive-PII; never surfaced in logs or in
    /// any consumer API that lacks a per-row audit trail.
    #[schema(example = "100123456")]
    pub number: String,
    /// ISO-8601 expiry date if any.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::domain::serde_helpers::iso_date_option")]
    #[schema(value_type = Option<String>, format = Date, example = "2035-12-31")]
    pub expiry: Option<time::Date>,
}

impl IdDocument {
    pub fn validate(&self) -> Result<(), ValueObjectError> {
        if self.issuer.trim().is_empty() {
            return Err(ValueObjectError::EmptyIdDocumentField("issuer"));
        }
        if self.issuer.chars().count() > 64 {
            return Err(ValueObjectError::IdDocumentFieldTooLong {
                field: "issuer",
                length: self.issuer.chars().count(),
                max: 64,
            });
        }
        if self.number.trim().is_empty() {
            return Err(ValueObjectError::EmptyIdDocumentField("number"));
        }
        if self.number.chars().count() > 64 {
            return Err(ValueObjectError::IdDocumentFieldTooLong {
                field: "number",
                length: self.number.chars().count(),
                max: 64,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdDocumentType {
    /// National identity card (Cameroonian CNI; equivalent abroad).
    NationalId,
    /// Passport.
    Passport,
    /// Driving licence.
    DrivingLicence,
    /// Residence card / titre de séjour.
    ResidenceCard,
    /// Other (declarant explains in supporting documents).
    Other,
}

impl IdDocumentType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NationalId => "national_id",
            Self::Passport => "passport",
            Self::DrivingLicence => "driving_licence",
            Self::ResidenceCard => "residence_card",
            Self::Other => "other",
        }
    }
}

/// The complete set of attributes a Person registration captures.
///
/// `date_of_birth` is optional to accommodate legacy beneficial-owner
/// records imported without a verified DOB (e.g. pre-platform paper
/// declarations). `biometric_reference_hash` is also optional;
/// enrolment ships behind a separate flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonAttributes {
    pub canonical_full_name: CanonicalFullName,
    pub nationality: Nationality,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::domain::serde_helpers::iso_date_option")]
    #[schema(value_type = Option<String>, format = Date, example = "1980-04-21")]
    pub date_of_birth: Option<time::Date>,
    pub primary_id_document: IdDocument,
    /// BLAKE3 or equivalent hash of a biometric template; **never**
    /// the template itself. Empty `None` indicates no biometric on
    /// file. Sensitive-PII per the classification matrix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "0f1a2b3c4d5e...")]
    pub biometric_reference_hash: Option<String>,
}

impl PersonAttributes {
    /// Validate the value objects nested inside the attributes. The
    /// individual newtypes (`Nationality`, `CanonicalFullName`)
    /// already validate at construction; this method covers the
    /// fields that ship as raw types (the `IdDocument` body, the
    /// optional biometric hash shape).
    pub fn validate(&self) -> Result<(), ValueObjectError> {
        self.primary_id_document.validate()?;
        if let Some(hash) = &self.biometric_reference_hash {
            let len = hash.chars().count();
            if !(64..=128).contains(&len) {
                return Err(ValueObjectError::InvalidBiometricHash {
                    length: len,
                });
            }
            if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ValueObjectError::InvalidBiometricHash {
                    length: len,
                });
            }
        }
        if let Some(dob) = self.date_of_birth {
            // Forward-dated DOBs are nonsensical for beneficial owners.
            let today = OffsetDateTime::now_utc().date();
            if dob > today {
                return Err(ValueObjectError::DateOfBirthInFuture {
                    date_of_birth: dob,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValueObjectError {
    #[error("invalid nationality `{given}`: {reason}")]
    InvalidNationality {
        given: String,
        reason: &'static str,
    },
    #[error("canonical_full_name must not be empty")]
    EmptyName,
    #[error("canonical_full_name length {length} exceeds maximum of {max} characters")]
    NameTooLong { length: usize, max: usize },
    #[error("id_document field `{0}` must not be empty")]
    EmptyIdDocumentField(&'static str),
    #[error("id_document field `{field}` length {length} exceeds maximum of {max}")]
    IdDocumentFieldTooLong {
        field: &'static str,
        length: usize,
        max: usize,
    },
    #[error("biometric_reference_hash must be 64..=128 hex chars; got length {length}")]
    InvalidBiometricHash { length: usize },
    #[error("date_of_birth {date_of_birth} is in the future (after today)")]
    DateOfBirthInFuture { date_of_birth: time::Date },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nationality_must_be_two_uppercase_letters() {
        assert!(Nationality::try_new("CM").is_ok());
        assert!(Nationality::try_new("FR").is_ok());
        assert!(Nationality::try_new("cm").is_err()); // lowercase
        assert!(Nationality::try_new("CMR").is_err()); // 3 chars
        assert!(Nationality::try_new("C").is_err()); // 1 char
        assert!(Nationality::try_new("").is_err());
        assert!(Nationality::try_new("12").is_err()); // digits
    }

    #[test]
    fn canonical_full_name_trims_and_rejects_empty() {
        let n = CanonicalFullName::try_new("  Ngono Marie  ").unwrap();
        assert_eq!(n.as_str(), "Ngono Marie");
        assert!(CanonicalFullName::try_new("").is_err());
        assert!(CanonicalFullName::try_new("   ").is_err());
    }

    #[test]
    fn canonical_full_name_rejects_overlong() {
        let too_long: String = "x".repeat(CanonicalFullName::MAX_LEN + 1);
        assert!(matches!(
            CanonicalFullName::try_new(too_long),
            Err(ValueObjectError::NameTooLong { .. })
        ));
    }

    #[test]
    fn id_document_validate_catches_empty_fields() {
        let mut doc = IdDocument {
            issuer: "CM:DGSN".into(),
            doc_type: IdDocumentType::NationalId,
            number: "123".into(),
            expiry: None,
        };
        assert!(doc.validate().is_ok());
        doc.issuer = "".into();
        assert!(doc.validate().is_err());
        doc.issuer = "CM:DGSN".into();
        doc.number = " ".into();
        assert!(doc.validate().is_err());
    }

    #[test]
    fn person_id_new_is_time_sortable() {
        let a = PersonId::new();
        let b = PersonId::new();
        assert!(b.0 > a.0, "uuidv7 must produce sortable identifiers");
    }
}
