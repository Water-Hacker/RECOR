//! Discriminated subject of a beneficial-ownership declaration.
//!
//! FATF R.24 vs R.25: a beneficial-ownership declaration can be about a
//! legal *entity* (R.24 — companies, NGOs, OHADA GIEs) OR about a legal
//! *arrangement* (R.25 — express trusts, fiducies, waqf, similar). The
//! Declaration aggregate carries one or the other, never both.
//!
//! Wire-shape evolution (TODO-002-declaration-link / ADR-0016):
//!
//!   - The legacy declaration wire shape uses a bare top-level
//!     `entity_id` field — every portal client in the field today
//!     produces that shape and signs canonical bytes over it. We
//!     CANNOT change those bytes without invalidating every signature
//!     a declarant has produced so far (D15 byte-parity is load-
//!     bearing).
//!
//!   - The NEW wire shape carries a tagged `subject: { kind, … }`
//!     discriminator. The portal will switch to this shape when
//!     arrangement submission lands. Until then, both shapes are
//!     accepted at the DTO boundary; legacy `entity_id` is mapped to
//!     `BeneficialOwnerSubject::Entity { entity_id }` server-side.
//!
//!   - The canonical bytes used for attestation are byte-parity-
//!     preserving: when the subject is `Entity`, the canonical form
//!     omits any `arrangement_id` field via `skip_serializing_if` and
//!     emits `entity_id` exactly where the legacy shape does. When the
//!     subject is `Arrangement`, the canonical form emits an
//!     `arrangement_id` field and OMITS `entity_id` entirely. Two
//!     separate `Canonical` structs are serialised — see
//!     `services/declaration/src/api/rest.rs::canonical_payload_bytes`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::value_object::EntityId;

/// Stable identifier for an R.25 arrangement (trust, fiducy, waqf,
/// similar). UUIDv7 — time-sortable so natural ordering matches
/// registration order. Mirrors the entity-service `ArrangementId`
/// type; the two services do not share a database so the cross-service
/// reference is unenforced at the SQL boundary (the v-engine validates
/// the reference resolves at verification time — D14 fail-closed).
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

impl fmt::Display for ArrangementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ArrangementId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Discriminator for the subject of a declaration. The aggregate
/// carries exactly one of the two variants; the SQL projection mirrors
/// the discriminator into the `subject_kind` column plus the
/// `entity_id` OR `arrangement_id` reference.
///
/// Serialisation uses the *tagged* form `{ kind: "entity"|"arrangement",
/// … }` — this is the NEW wire shape that the portal adopts when
/// arrangement submission lands. The legacy bare `entity_id` shape is
/// accepted at the DTO boundary and converted to `Entity { entity_id }`
/// server-side; see `api::dto::SubmitDeclarationRequest::resolve_subject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BeneficialOwnerSubject {
    /// FATF R.24 — the declaration is about a legal entity.
    Entity {
        #[schema(value_type = String, format = "uuid")]
        entity_id: EntityId,
    },
    /// FATF R.25 — the declaration is about a legal arrangement (trust,
    /// fiducy, waqf, similar). The `arrangement_id` resolves against
    /// the entity-service `arrangements` table; the verification engine
    /// validates the reference exists at verification time (D14 fail-
    /// closed: an unknown arrangement_id surfaces as a verification
    /// failure, never a silent admission).
    Arrangement {
        #[schema(value_type = String, format = "uuid")]
        arrangement_id: ArrangementId,
    },
}

impl BeneficialOwnerSubject {
    /// Storage discriminator — matches the
    /// `declarations.subject_kind` column's CHECK constraint
    /// (migration 0015).
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Entity { .. } => "entity",
            Self::Arrangement { .. } => "arrangement",
        }
    }

    /// Returns the entity_id when the subject is Entity. Used by the
    /// SQL projection writer (`subject_kind='entity'` rows carry
    /// `entity_id` and NULL `arrangement_id`).
    pub fn entity_id(&self) -> Option<EntityId> {
        match self {
            Self::Entity { entity_id } => Some(*entity_id),
            Self::Arrangement { .. } => None,
        }
    }

    /// Returns the arrangement_id when the subject is Arrangement.
    pub fn arrangement_id(&self) -> Option<ArrangementId> {
        match self {
            Self::Arrangement { arrangement_id } => Some(*arrangement_id),
            Self::Entity { .. } => None,
        }
    }

    /// Convenience constructor — wraps an EntityId in the Entity
    /// variant. The DTO conversion uses this when the wire shape
    /// carries only the legacy bare `entity_id`.
    pub fn from_entity_id(entity_id: EntityId) -> Self {
        Self::Entity { entity_id }
    }

    /// Convenience constructor — wraps an ArrangementId in the
    /// Arrangement variant.
    pub fn from_arrangement_id(arrangement_id: ArrangementId) -> Self {
        Self::Arrangement { arrangement_id }
    }

    /// Returns true when the subject is an R.25 arrangement. Used by
    /// the cascade-tier resolver to switch between R.24 §c.24.6
    /// cascade and R.25 settlor→trustee→protector→beneficiary chain.
    pub fn is_arrangement(&self) -> bool {
        matches!(self, Self::Arrangement { .. })
    }
}

impl fmt::Display for BeneficialOwnerSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entity { entity_id } => write!(f, "entity:{entity_id}"),
            Self::Arrangement { arrangement_id } => {
                write!(f, "arrangement:{arrangement_id}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn entity_subject_serialises_with_tagged_kind() {
        let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::nil()));
        let json = serde_json::to_string(&subj).unwrap();
        assert!(json.contains(r#""kind":"entity""#));
        assert!(json.contains(r#""entity_id":"00000000-0000-0000-0000-000000000000""#));
        // The arrangement_id field MUST NOT appear on the Entity variant.
        assert!(!json.contains("arrangement_id"));
    }

    #[test]
    fn arrangement_subject_serialises_with_tagged_kind() {
        let subj = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::nil()));
        let json = serde_json::to_string(&subj).unwrap();
        assert!(json.contains(r#""kind":"arrangement""#));
        assert!(json.contains(r#""arrangement_id":"00000000-0000-0000-0000-000000000000""#));
        // The entity_id field MUST NOT appear on the Arrangement variant.
        assert!(!json.contains("entity_id"));
    }

    #[test]
    fn entity_subject_roundtrips_through_serde() {
        let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::now_v7()));
        let json = serde_json::to_string(&subj).unwrap();
        let back: BeneficialOwnerSubject = serde_json::from_str(&json).unwrap();
        assert_eq!(subj, back);
    }

    #[test]
    fn arrangement_subject_roundtrips_through_serde() {
        let subj = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::now_v7()));
        let json = serde_json::to_string(&subj).unwrap();
        let back: BeneficialOwnerSubject = serde_json::from_str(&json).unwrap();
        assert_eq!(subj, back);
    }

    #[test]
    fn kind_str_matches_migration_check_constraint() {
        // Migration 0015's CHECK constraint admits exactly these two
        // discriminator strings — anything else would 23514 the
        // INSERT. The aggregate's storage writer MUST therefore emit
        // exactly one of these.
        let e = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::nil()));
        assert_eq!(e.kind_str(), "entity");
        let a = BeneficialOwnerSubject::from_arrangement_id(ArrangementId(Uuid::nil()));
        assert_eq!(a.kind_str(), "arrangement");
    }

    #[test]
    fn arrangement_id_from_str_parses_uuid() {
        let a: ArrangementId = "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d".parse().unwrap();
        assert_eq!(a.to_string(), "0192f1d4-1e0a-7c4b-9b1e-3d4f5a6b7c8d");
    }

    #[test]
    fn arrangement_id_from_str_rejects_garbage() {
        let r: Result<ArrangementId, _> = "not-a-uuid".parse();
        assert!(r.is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn entity_subject_roundtrips_property(bytes in any::<[u8; 16]>()) {
            let subj = BeneficialOwnerSubject::from_entity_id(EntityId(Uuid::from_bytes(bytes)));
            let json = serde_json::to_string(&subj).unwrap();
            let back: BeneficialOwnerSubject = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(subj, back);
        }

        #[test]
        fn arrangement_subject_roundtrips_property(bytes in any::<[u8; 16]>()) {
            let subj = BeneficialOwnerSubject::from_arrangement_id(
                ArrangementId(Uuid::from_bytes(bytes)),
            );
            let json = serde_json::to_string(&subj).unwrap();
            let back: BeneficialOwnerSubject = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(subj, back);
        }
    }
}
