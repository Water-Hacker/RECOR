//! Domain value objects for the FATF R.25 / INR.25 *Arrangement* aggregate.
//!
//! Mirrors the shape of `value_object.rs` (legal entities) — newtype-wrapped
//! primitives that carry domain meaning the underlying type does not. Each
//! type is constructed through a fallible `try_from_*` and validates at the
//! boundary so the aggregate can trust its inputs thereafter.
//!
//! The R.25 schema obligations (see ADR-0015 + migration 0003) name six
//! identifier roles; the JSONB shape captured here lets a single arrangement
//! hold multiple references per role without spawning a join table.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Stable identifier for an R.25 arrangement. UUIDv7 — time-sortable so
/// the natural ordering matches registration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(
    value_type = String,
    format = "uuid",
    example = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d"
)]
pub struct ArrangementId(pub Uuid);

impl ArrangementId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for ArrangementId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ArrangementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The four canonical R.25 arrangement kinds. The CHECK constraint on
/// `arrangements.arrangement_kind` (migration 0003) mirrors this set;
/// a new kind requires both an enum variant AND a migration that
/// updates the CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrangementKind {
    /// Common-law express trust (or its civil-law equivalents).
    ExpressTrust,
    /// OHADA / Civil-law fiducie / fideicomisos.
    Fiducy,
    /// Islamic charitable / family endowment (waqf).
    Waqf,
    /// R.25-similar catch-all (Liechtenstein Anstalt, STAR trusts, …).
    Similar,
}

impl ArrangementKind {
    pub fn as_storage_str(self) -> &'static str {
        match self {
            Self::ExpressTrust => "express_trust",
            Self::Fiducy => "fiducy",
            Self::Waqf => "waqf",
            Self::Similar => "similar",
        }
    }

    pub fn try_from_storage_str(s: &str) -> Result<Self, ArrangementValueObjectError> {
        match s {
            "express_trust" => Ok(Self::ExpressTrust),
            "fiducy" => Ok(Self::Fiducy),
            "waqf" => Ok(Self::Waqf),
            "similar" => Ok(Self::Similar),
            other => Err(ArrangementValueObjectError::UnknownArrangementKind(
                other.to_string(),
            )),
        }
    }
}

/// One reference inside a `settlor_refs` JSONB cell — settlors are
/// statutorily natural persons (R.25 INR §3.a names "the settlor"), so
/// the only admissible target is a `person_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SettlorRef {
    pub person_id: Uuid,
    /// Free-form qualifier — "co-settlor", "originator", …. Optional;
    /// when present, 1..=128 trimmed characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

/// One reference inside a `trustee_refs` JSONB cell. R.25 admits three
/// shapes: natural-person trustee, legal-person trustee, or a registered
/// fiduciary (TCSP, notary). Exactly one of the three fields MUST be set
/// per entry; the validator enforces this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TrusteeRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<Uuid>,
    /// External registration handle for a regulated fiduciary that is
    /// neither a registered legal entity in this service nor a natural
    /// person (e.g. an offshore TCSP). 1..=128 trimmed characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiduciary_registration_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

impl TrusteeRef {
    /// Validate the *exactly-one-of* invariant. Returns the discriminator
    /// label for use in diagnostics.
    pub fn validate(&self) -> Result<&'static str, ArrangementValueObjectError> {
        let n = [
            self.person_id.is_some(),
            self.entity_id.is_some(),
            self.fiduciary_registration_id.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        if n != 1 {
            return Err(ArrangementValueObjectError::TrusteeRefShape(n));
        }
        if let Some(fid) = self.fiduciary_registration_id.as_deref() {
            let trimmed = fid.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 128 {
                return Err(ArrangementValueObjectError::FiduciaryRegistrationIdInvalid);
            }
        }
        if let Some(meta) = self.role_metadata.as_deref() {
            if meta.chars().count() > 128 {
                return Err(ArrangementValueObjectError::RoleMetadataTooLong);
            }
        }
        Ok(if self.person_id.is_some() {
            "person"
        } else if self.entity_id.is_some() {
            "entity"
        } else {
            "fiduciary"
        })
    }
}

/// `protector_refs` JSONB cell — R.25 admits a protector only as a
/// natural person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProtectorRef {
    pub person_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

/// `named_beneficiary_refs` JSONB cell — beneficiaries named in the
/// trust deed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NamedBeneficiaryRef {
    pub person_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_metadata: Option<String>,
}

/// `class_beneficiary_specs` JSONB cell — class-defined beneficiaries.
/// R.25 admits open-class trust beneficiaries ("my grandchildren") so
/// the structure captures the class label plus an arbitrary criteria
/// object the investigator can later resolve against named individuals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClassBeneficiarySpec {
    /// Short label — "grandchildren", "employees-with-tenure-≥10y", …
    pub class: String,
    /// Free-form structured criteria. The JSON shape is open by design;
    /// the API DTO carries it as a `serde_json::Value` so a future
    /// schema extension does not require a migration.
    pub criteria: serde_json::Value,
}

/// `control_exercise_refs` JSONB cell — R.25 catch-all for "any other
/// natural persons exercising ultimate effective control". The
/// `control_basis` is free-form text the investigator may use; the
/// aggregate enforces non-empty length only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ControlExerciseRef {
    pub person_id: Uuid,
    pub control_basis: String,
}

/// Validated free-form jurisdiction string. Reuses the legal-entity
/// `Jurisdiction` shape: ISO-3166-1 alpha-2, uppercase, exactly 2
/// ASCII letters.
pub use super::value_object::Jurisdiction as GoverningLawJurisdiction;

/// The set of fields editable in-place on an existing arrangement.
///
/// Identity fields (`arrangement_id`, `arrangement_kind`,
/// `governing_law_jurisdiction`, `constitution_date`) are NOT in this
/// set: changing them would create a new arrangement, not modify the
/// existing one. Dissolution lives on its own command. Every other
/// role-reference column IS editable here — the back-office workflow
/// often adds successor trustees, names new beneficiaries, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ArrangementUpdatableFields {
    pub settlor_refs: Vec<SettlorRef>,
    pub trustee_refs: Vec<TrusteeRef>,
    pub protector_refs: Vec<ProtectorRef>,
    pub named_beneficiary_refs: Vec<NamedBeneficiaryRef>,
    pub class_beneficiary_specs: Vec<ClassBeneficiarySpec>,
    pub control_exercise_refs: Vec<ControlExerciseRef>,
}

impl ArrangementUpdatableFields {
    /// Validate the FATF R.25 invariants the *value-object* layer is
    /// responsible for: every collection's individual entries are
    /// well-formed. Aggregate-level invariants ("≥ 1 settlor, ≥ 1
    /// trustee") live on the aggregate.
    pub fn validate(&self) -> Result<(), ArrangementValueObjectError> {
        for t in &self.trustee_refs {
            t.validate()?;
        }
        for c in &self.control_exercise_refs {
            if c.control_basis.trim().is_empty() {
                return Err(ArrangementValueObjectError::EmptyControlBasis);
            }
            if c.control_basis.chars().count() > 512 {
                return Err(ArrangementValueObjectError::ControlBasisTooLong);
            }
        }
        for s in &self.class_beneficiary_specs {
            if s.class.trim().is_empty() {
                return Err(ArrangementValueObjectError::EmptyClassLabel);
            }
            if s.class.chars().count() > 256 {
                return Err(ArrangementValueObjectError::ClassLabelTooLong);
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArrangementValueObjectError {
    #[error("unknown arrangement kind `{0}`; expected one of: express_trust, fiducy, waqf, similar")]
    UnknownArrangementKind(String),
    #[error("trustee ref has {0} discriminators set; exactly one of person_id, entity_id, fiduciary_registration_id is required")]
    TrusteeRefShape(usize),
    #[error("fiduciary_registration_id must be 1..=128 trimmed characters")]
    FiduciaryRegistrationIdInvalid,
    #[error("role_metadata must be ≤ 128 characters")]
    RoleMetadataTooLong,
    #[error("control_basis is empty after trimming")]
    EmptyControlBasis,
    #[error("control_basis exceeds 512 characters")]
    ControlBasisTooLong,
    #[error("class beneficiary class label is empty after trimming")]
    EmptyClassLabel,
    #[error("class beneficiary class label exceeds 256 characters")]
    ClassLabelTooLong,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn fresh_uuid() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn arrangement_kind_round_trips_through_storage() {
        for k in [
            ArrangementKind::ExpressTrust,
            ArrangementKind::Fiducy,
            ArrangementKind::Waqf,
            ArrangementKind::Similar,
        ] {
            let stored = k.as_storage_str();
            let parsed = ArrangementKind::try_from_storage_str(stored).unwrap();
            assert_eq!(k, parsed);
        }
    }

    #[test]
    fn arrangement_kind_rejects_unknown() {
        let err = ArrangementKind::try_from_storage_str("bare_trust").unwrap_err();
        assert!(matches!(
            err,
            ArrangementValueObjectError::UnknownArrangementKind(_)
        ));
    }

    #[test]
    fn trustee_ref_requires_exactly_one_discriminator() {
        let none = TrusteeRef {
            person_id: None,
            entity_id: None,
            fiduciary_registration_id: None,
            role_metadata: None,
        };
        assert!(none.validate().is_err());

        let both = TrusteeRef {
            person_id: Some(fresh_uuid()),
            entity_id: Some(fresh_uuid()),
            fiduciary_registration_id: None,
            role_metadata: None,
        };
        assert!(both.validate().is_err());

        let person_only = TrusteeRef {
            person_id: Some(fresh_uuid()),
            entity_id: None,
            fiduciary_registration_id: None,
            role_metadata: None,
        };
        assert_eq!(person_only.validate().unwrap(), "person");

        let entity_only = TrusteeRef {
            person_id: None,
            entity_id: Some(fresh_uuid()),
            fiduciary_registration_id: None,
            role_metadata: None,
        };
        assert_eq!(entity_only.validate().unwrap(), "entity");

        let fid_only = TrusteeRef {
            person_id: None,
            entity_id: None,
            fiduciary_registration_id: Some("TCSP/CH/Z/12345".to_string()),
            role_metadata: None,
        };
        assert_eq!(fid_only.validate().unwrap(), "fiduciary");
    }

    #[test]
    fn trustee_ref_rejects_empty_fiduciary_handle() {
        let t = TrusteeRef {
            person_id: None,
            entity_id: None,
            fiduciary_registration_id: Some("   ".to_string()),
            role_metadata: None,
        };
        // empty-after-trim is counted as "not set" by the count branch,
        // so the shape error fires first.
        let err = t.validate().unwrap_err();
        assert!(matches!(err, ArrangementValueObjectError::TrusteeRefShape(_)));
    }

    #[test]
    fn trustee_ref_rejects_overlong_metadata() {
        let t = TrusteeRef {
            person_id: Some(fresh_uuid()),
            entity_id: None,
            fiduciary_registration_id: None,
            role_metadata: Some("x".repeat(129)),
        };
        let err = t.validate().unwrap_err();
        assert!(matches!(err, ArrangementValueObjectError::RoleMetadataTooLong));
    }

    #[test]
    fn updatable_fields_validate_control_basis() {
        let fields = ArrangementUpdatableFields {
            settlor_refs: vec![],
            trustee_refs: vec![],
            protector_refs: vec![],
            named_beneficiary_refs: vec![],
            class_beneficiary_specs: vec![],
            control_exercise_refs: vec![ControlExerciseRef {
                person_id: fresh_uuid(),
                control_basis: "   ".to_string(),
            }],
        };
        assert!(matches!(
            fields.validate().unwrap_err(),
            ArrangementValueObjectError::EmptyControlBasis
        ));
    }

    // ─── Proptest — role-reference JSONB round-trip ─────────────────
    //
    // Each role-reference type carries an explicit JSONB shape that is
    // serialised to/from Postgres. The aggregate's invariants depend on
    // that shape being lossless; the property test below generates
    // randomised values for every variant of the discriminator
    // (`TrusteeRef::{person|entity|fiduciary}`) and verifies that the
    // serde round-trip matches the original. The test is intentionally
    // generous on the input space — the canonical-form contract for
    // JSONB row storage must hold for every valid TrusteeRef.

    use proptest::prelude::*;

    fn trustee_ref_strategy() -> impl Strategy<Value = TrusteeRef> {
        let person_only = prop::strategy::Just(()).prop_map(|_| TrusteeRef {
            person_id: Some(Uuid::now_v7()),
            entity_id: None,
            fiduciary_registration_id: None,
            role_metadata: None,
        });
        let entity_only = prop::strategy::Just(()).prop_map(|_| TrusteeRef {
            person_id: None,
            entity_id: Some(Uuid::now_v7()),
            fiduciary_registration_id: None,
            role_metadata: None,
        });
        // Restrict to alphanumerics + slash/dash; 1..=64 chars (well
        // below the 128-character ceiling).
        let fiduciary_only =
            "[A-Za-z0-9/_-]{1,64}".prop_map(|fid| TrusteeRef {
                person_id: None,
                entity_id: None,
                fiduciary_registration_id: Some(fid),
                role_metadata: None,
            });
        prop_oneof![person_only, entity_only, fiduciary_only]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn trustee_ref_jsonb_round_trips(t in trustee_ref_strategy()) {
            // Validation must pass on the generated input.
            let label = t.validate().expect("strategy yields well-formed TrusteeRef");
            // serde_json round-trip preserves the exact shape (the
            // JSONB storage path uses `serde_json::to_value` /
            // `serde_json::from_value` on the same struct).
            let v = serde_json::to_value(&t).expect("serialise");
            let back: TrusteeRef = serde_json::from_value(v).expect("deserialise");
            let back_label = back.validate().expect("round-tripped value validates");
            prop_assert_eq!(label, back_label);
            prop_assert_eq!(t, back);
        }
    }

    #[test]
    fn updatable_fields_validate_class_labels() {
        let fields = ArrangementUpdatableFields {
            settlor_refs: vec![],
            trustee_refs: vec![],
            protector_refs: vec![],
            named_beneficiary_refs: vec![],
            class_beneficiary_specs: vec![ClassBeneficiarySpec {
                class: "".to_string(),
                criteria: json!({}),
            }],
            control_exercise_refs: vec![],
        };
        assert!(matches!(
            fields.validate().unwrap_err(),
            ArrangementValueObjectError::EmptyClassLabel
        ));
    }
}
