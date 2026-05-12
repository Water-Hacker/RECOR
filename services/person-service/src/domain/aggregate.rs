//! Person aggregate. Event-sourced.
//!
//! `PersonAggregate` is the unit of consistency. Commands are
//! validated against current aggregate state; valid commands produce
//! events; events are applied to update state.
//!
//! Invariants enforced here:
//!   - `person_id` may receive a Register command only once.
//!   - Updates are refused once the person has been merged into another.
//!   - Merges are strictly linear: a merged-out person cannot be merged
//!     again, and the target of a merge cannot itself have been merged.
//!   - A person cannot be merged into themselves.
//!   - `actor_principal` is non-empty on every command.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::command::{MergePersons, RegisterPerson, UpdatePerson};
use super::error::DomainError;
use super::event::{PersonEvent, PersonMergedV1, PersonRegisteredV1, PersonUpdatedV1};
use super::value_object::{PersonAttributes, PersonId};

/// In-memory representation of a Person aggregate, hydrated from
/// its event stream. Same shape mirror as
/// `services/declaration::DeclarationAggregate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonAggregate {
    pub id: PersonId,
    /// Monotonic event count, used for optimistic concurrency.
    pub version: u64,
    /// `None` until the first Registered event has been applied.
    pub attributes: Option<PersonAttributes>,
    /// If this person has been merged into another, the target id.
    /// `None` for a live aggregate.
    pub merged_into: Option<PersonId>,
}

impl PersonAggregate {
    /// Construct a fresh aggregate at version 0, no events applied yet.
    pub fn fresh(id: PersonId) -> Self {
        Self {
            id,
            version: 0,
            attributes: None,
            merged_into: None,
        }
    }

    /// Hydrate by replaying events in order.
    pub fn from_events(id: PersonId, events: &[PersonEvent]) -> Self {
        let mut agg = Self::fresh(id);
        for event in events {
            agg.apply(event);
        }
        agg
    }

    /// Apply an event to advance state. Pure; no I/O.
    pub fn apply(&mut self, event: &PersonEvent) {
        match event {
            PersonEvent::Registered(p) => {
                self.attributes = Some(p.attributes.clone());
                self.merged_into = None;
            }
            PersonEvent::Updated(p) => {
                self.attributes = Some(p.after.clone());
            }
            PersonEvent::Merged(p) => {
                self.merged_into = Some(p.into_person_id);
            }
        }
        self.version = self.version.saturating_add(1);
    }

    /// Validate a Register command and produce the resulting event.
    /// Does NOT mutate `self`; the caller decides whether to apply
    /// after persistence succeeds.
    pub fn handle_register(
        &self,
        cmd: RegisterPerson,
    ) -> Result<PersonEvent, DomainError> {
        if self.version > 0 {
            return Err(DomainError::AlreadyRegistered(self.id.0));
        }
        validate_actor(&cmd.actor_principal)?;
        cmd.attributes.validate()?;

        let payload = PersonRegisteredV1 {
            person_id: cmd.person_id,
            attributes: cmd.attributes,
            actor_principal: cmd.actor_principal,
            registered_at: cmd.registered_at,
            correlation_id: cmd.correlation_id,
        };
        Ok(PersonEvent::Registered(payload))
    }

    /// Validate an Update command and produce an event.
    ///
    /// Rules enforced here:
    ///   - Aggregate must have a prior Registered event (version > 0).
    ///   - Aggregate must not have been merged into another.
    ///   - `actor_principal` is non-empty.
    ///   - The new attributes pass `PersonAttributes::validate`.
    pub fn handle_update(
        &self,
        cmd: UpdatePerson,
    ) -> Result<PersonEvent, DomainError> {
        if self.version == 0 {
            return Err(DomainError::UpdateBeforeRegister(self.id.0));
        }
        if let Some(target) = self.merged_into {
            return Err(DomainError::AlreadyMerged {
                person_id: self.id.0,
                into: target.0,
            });
        }
        validate_actor(&cmd.actor_principal)?;
        cmd.attributes.validate()?;

        // before snapshot — the aggregate's current attributes (always
        // populated once Registered has been applied).
        let before = self.attributes.clone().ok_or_else(|| {
            DomainError::UpdateBeforeRegister(self.id.0)
        })?;

        let payload = PersonUpdatedV1 {
            person_id: cmd.person_id,
            before,
            after: cmd.attributes,
            actor_principal: cmd.actor_principal,
            updated_at: cmd.updated_at,
            correlation_id: cmd.correlation_id,
        };
        Ok(PersonEvent::Updated(payload))
    }

    /// Validate a Merge command against THIS aggregate (the FROM /
    /// duplicate). Produces a `PersonMergedV1` event for THIS aggregate.
    ///
    /// `target_already_merged` is the caller's pre-check of whether the
    /// merge target is itself a merged-out shell (the application layer
    /// loads the target aggregate to surface this).
    ///
    /// Rules:
    ///   - Aggregate must exist (version > 0).
    ///   - Aggregate must not already be merged into another.
    ///   - Target cannot be the same as the source.
    ///   - Target must not itself have been merged (linear merge chain).
    ///   - `actor_principal` is non-empty.
    pub fn handle_merge(
        &self,
        cmd: MergePersons,
        target_already_merged: bool,
    ) -> Result<PersonEvent, DomainError> {
        if self.version == 0 {
            return Err(DomainError::MergeBeforeRegister(self.id.0));
        }
        if let Some(existing) = self.merged_into {
            return Err(DomainError::AlreadyMerged {
                person_id: self.id.0,
                into: existing.0,
            });
        }
        if cmd.from_person_id == cmd.into_person_id {
            return Err(DomainError::MergeIntoSelf(self.id.0));
        }
        if target_already_merged {
            return Err(DomainError::MergeTargetIsMerged(cmd.into_person_id.0));
        }
        validate_actor(&cmd.actor_principal)?;

        let payload = PersonMergedV1 {
            person_id: self.id,
            into_person_id: cmd.into_person_id,
            actor_principal: cmd.actor_principal,
            merged_at: cmd.merged_at,
            correlation_id: cmd.correlation_id,
        };
        Ok(PersonEvent::Merged(payload))
    }
}

fn validate_actor(principal: &str) -> Result<(), DomainError> {
    if principal.trim().is_empty() {
        return Err(DomainError::EmptyActorPrincipal);
    }
    Ok(())
}

// Reserved for future use — convenience constructor.
#[allow(dead_code)]
fn now_utc() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

#[cfg(test)]
mod tests {
    use time::macros::date;
    use uuid::Uuid;

    use crate::domain::value_object::{
        CanonicalFullName, IdDocument, IdDocumentType, Nationality, PersonAttributes, PersonId,
    };

    use super::*;

    fn attributes(name: &str) -> PersonAttributes {
        PersonAttributes {
            canonical_full_name: CanonicalFullName::try_new(name).unwrap(),
            nationality: Nationality::try_new("CM").unwrap(),
            date_of_birth: Some(date!(1980 - 04 - 21)),
            primary_id_document: IdDocument {
                issuer: "CM:DGSN".into(),
                doc_type: IdDocumentType::NationalId,
                number: "100123456".into(),
                expiry: Some(date!(2035 - 12 - 31)),
            },
            biometric_reference_hash: None,
        }
    }

    fn register_command(id: PersonId, name: &str) -> RegisterPerson {
        RegisterPerson {
            person_id: id,
            attributes: attributes(name),
            actor_principal: "spiffe://recor.cm/test-operator".to_string(),
            registered_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn update_command(id: PersonId, name: &str) -> UpdatePerson {
        UpdatePerson {
            person_id: id,
            attributes: attributes(name),
            actor_principal: "spiffe://recor.cm/test-operator".to_string(),
            updated_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn merge_command(from: PersonId, into: PersonId) -> MergePersons {
        MergePersons {
            from_person_id: from,
            into_person_id: into,
            actor_principal: "spiffe://recor.cm/admin".to_string(),
            merged_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn fresh_aggregate_at_version_zero() {
        let agg = PersonAggregate::fresh(PersonId::new());
        assert_eq!(agg.version, 0);
        assert!(agg.attributes.is_none());
    }

    #[test]
    fn register_increments_version_and_seeds_attributes() {
        let id = PersonId::new();
        let agg = PersonAggregate::fresh(id);
        let event = agg
            .handle_register(register_command(id, "Ngono Marie"))
            .expect("valid register");
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.version, 1);
        assert_eq!(
            agg.attributes.unwrap().canonical_full_name.as_str(),
            "Ngono Marie"
        );
    }

    #[test]
    fn register_twice_is_rejected() {
        let id = PersonId::new();
        let mut agg = PersonAggregate::fresh(id);
        let event = agg
            .handle_register(register_command(id, "Ngono Marie"))
            .unwrap();
        agg.apply(&event);
        let err = agg
            .handle_register(register_command(id, "Ngono Marie 2"))
            .unwrap_err();
        assert!(matches!(err, DomainError::AlreadyRegistered(_)));
    }

    #[test]
    fn empty_actor_principal_rejected_on_register() {
        let id = PersonId::new();
        let agg = PersonAggregate::fresh(id);
        let mut cmd = register_command(id, "Ngono Marie");
        cmd.actor_principal = "  ".to_string();
        assert_eq!(
            agg.handle_register(cmd).unwrap_err(),
            DomainError::EmptyActorPrincipal
        );
    }

    #[test]
    fn update_before_register_rejected() {
        let id = PersonId::new();
        let agg = PersonAggregate::fresh(id);
        let err = agg
            .handle_update(update_command(id, "Tchami Paul"))
            .unwrap_err();
        assert!(matches!(err, DomainError::UpdateBeforeRegister(_)));
    }

    #[test]
    fn update_after_register_emits_event_with_before_snapshot() {
        let id = PersonId::new();
        let mut agg = PersonAggregate::fresh(id);
        let r = agg
            .handle_register(register_command(id, "Ngono Marie"))
            .unwrap();
        agg.apply(&r);
        let u = agg.handle_update(update_command(id, "Ngono Marie-Claire")).unwrap();
        let PersonEvent::Updated(payload) = u else {
            panic!("expected Updated event");
        };
        assert_eq!(payload.before.canonical_full_name.as_str(), "Ngono Marie");
        assert_eq!(
            payload.after.canonical_full_name.as_str(),
            "Ngono Marie-Claire"
        );
    }

    #[test]
    fn update_after_merge_rejected() {
        let from = PersonId::new();
        let into = PersonId::new();
        // Hydrate both with Registered events.
        let mut from_agg = PersonAggregate::fresh(from);
        from_agg.apply(&from_agg.handle_register(register_command(from, "Dup A")).unwrap());
        let mut into_agg = PersonAggregate::fresh(into);
        into_agg.apply(&into_agg.handle_register(register_command(into, "Canon B")).unwrap());
        // Merge from → into.
        let merge_event = from_agg
            .handle_merge(merge_command(from, into), false)
            .unwrap();
        from_agg.apply(&merge_event);
        // Attempt to update the merged-out aggregate.
        let err = from_agg.handle_update(update_command(from, "x")).unwrap_err();
        assert!(matches!(err, DomainError::AlreadyMerged { .. }));
    }

    #[test]
    fn merge_into_self_rejected() {
        let id = PersonId::new();
        let mut agg = PersonAggregate::fresh(id);
        agg.apply(&agg.handle_register(register_command(id, "Solo")).unwrap());
        let err = agg.handle_merge(merge_command(id, id), false).unwrap_err();
        assert!(matches!(err, DomainError::MergeIntoSelf(_)));
    }

    #[test]
    fn merge_target_must_not_be_merged() {
        let from = PersonId::new();
        let into = PersonId::new();
        let mut from_agg = PersonAggregate::fresh(from);
        from_agg.apply(
            &from_agg
                .handle_register(register_command(from, "Dup"))
                .unwrap(),
        );
        // Target is "already merged" — caller-supplied flag (linearity check).
        let err = from_agg
            .handle_merge(merge_command(from, into), true)
            .unwrap_err();
        assert!(matches!(err, DomainError::MergeTargetIsMerged(_)));
    }

    #[test]
    fn merge_then_merge_again_rejected() {
        let from = PersonId::new();
        let into_a = PersonId::new();
        let into_b = PersonId::new();
        let mut from_agg = PersonAggregate::fresh(from);
        from_agg.apply(&from_agg.handle_register(register_command(from, "Dup")).unwrap());
        // First merge.
        let event = from_agg
            .handle_merge(merge_command(from, into_a), false)
            .unwrap();
        from_agg.apply(&event);
        // Second merge attempt against the same aggregate.
        let err = from_agg
            .handle_merge(merge_command(from, into_b), false)
            .unwrap_err();
        assert!(matches!(err, DomainError::AlreadyMerged { .. }));
    }

    #[test]
    fn replay_reproduces_state_post_update() {
        let id = PersonId::new();
        let mut agg = PersonAggregate::fresh(id);
        let reg = agg.handle_register(register_command(id, "Ngono")).unwrap();
        agg.apply(&reg);
        let upd = agg.handle_update(update_command(id, "Ngono II")).unwrap();
        agg.apply(&upd);

        let replayed = PersonAggregate::from_events(id, &[reg, upd]);
        assert_eq!(replayed.version, 2);
        assert_eq!(
            replayed.attributes.unwrap().canonical_full_name.as_str(),
            "Ngono II"
        );
    }

    #[test]
    fn invalid_nationality_caught_at_attributes_validate() {
        // Direct constructor refuses, so synthesise via try_from to
        // confirm the value-object error surfaces.
        let err = Nationality::try_new("xx").unwrap_err();
        assert!(matches!(
            err,
            crate::domain::value_object::ValueObjectError::InvalidNationality { .. }
        ));
    }
}
