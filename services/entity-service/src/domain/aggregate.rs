//! Entity aggregate. Event-sourced; commands produce events; events are
//! applied (folded) to derive the current state.
//!
//! Lifecycle: absent → Active (Registered) → Dissolved (Dissolved).
//! Updates are permitted only while Active.
//!
//! The aggregate is pure: no I/O, no logging, no async. It owns its
//! invariants and surfaces violations as `DomainError`.

use time::OffsetDateTime;

use super::command::{DissolveEntity, RegisterEntity, UpdateEntity};
use super::error::DomainError;
use super::event::{EntityDissolvedV1, EntityEvent, EntityRegisteredV1, EntityUpdatedV1};
use super::value_object::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegistrationNumber, UpdatableFields,
};

/// In-memory aggregate state — the materialised result of folding the
/// event log. The Postgres projection mirrors this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityAggregate {
    pub id: EntityId,
    pub state: EntityState,
    /// `0` means "no events yet" (absent). Each applied event increments
    /// the version. Used by the repository for optimistic concurrency.
    pub version: u64,
    pub snapshot: Option<EntitySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityState {
    Absent,
    Active,
    Dissolved,
}

/// Materialised projection of the aggregate's current fields. `None`
/// when the aggregate is absent; populated from the Registered event
/// onward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySnapshot {
    pub canonical_name: CanonicalName,
    pub entity_type: EntityType,
    pub jurisdiction: Jurisdiction,
    pub registration_number_in_jurisdiction: RegistrationNumber,
    pub founded_at: time::Date,
    pub dissolved_at: Option<time::Date>,
}

impl EntityAggregate {
    /// Build an aggregate from its event log. An empty event list
    /// yields the `Absent` state at version 0.
    pub fn from_events(id: EntityId, events: &[EntityEvent]) -> Self {
        let mut agg = Self {
            id,
            state: EntityState::Absent,
            version: 0,
            snapshot: None,
        };
        for e in events {
            agg.apply(e);
        }
        agg
    }

    /// Fold an event into the in-memory state. Always increments
    /// version. Defensive: events for the wrong aggregate id are
    /// silently ignored at this layer; the repository contract refuses
    /// to load them in the first place.
    pub fn apply(&mut self, event: &EntityEvent) {
        if event.entity_id() != self.id {
            return;
        }
        match event {
            EntityEvent::Registered(p) => {
                self.state = EntityState::Active;
                self.snapshot = Some(EntitySnapshot {
                    canonical_name: p.canonical_name.clone(),
                    entity_type: p.entity_type.clone(),
                    jurisdiction: p.jurisdiction.clone(),
                    registration_number_in_jurisdiction: p
                        .registration_number_in_jurisdiction
                        .clone(),
                    founded_at: p.founded_at,
                    dissolved_at: None,
                });
            }
            EntityEvent::Updated(p) => {
                if let Some(snap) = self.snapshot.as_mut() {
                    snap.canonical_name = p.after.canonical_name.clone();
                    snap.entity_type = p.after.entity_type.clone();
                }
            }
            EntityEvent::Dissolved(p) => {
                self.state = EntityState::Dissolved;
                if let Some(snap) = self.snapshot.as_mut() {
                    snap.dissolved_at = Some(p.dissolved_at);
                }
            }
        }
        self.version = self.version.saturating_add(1);
    }

    /// Handle a Register command. Refuses if the aggregate already
    /// exists; otherwise emits `EntityRegisteredV1`.
    pub fn handle_register(
        &self,
        cmd: RegisterEntity,
        now_utc: OffsetDateTime,
    ) -> Result<EntityEvent, DomainError> {
        if !matches!(self.state, EntityState::Absent) {
            return Err(DomainError::AlreadyRegistered(self.id.0));
        }
        if cmd.entity_id != self.id {
            // Defensive: the use case sets the aggregate id from the
            // command before construction, so this branch implies a
            // wiring bug. We surface it as a domain error rather than
            // panic.
            return Err(DomainError::AlreadyRegistered(self.id.0));
        }
        let today = now_utc.date();
        if cmd.founded_at > today {
            return Err(DomainError::FoundedAtInFuture {
                founded_at: cmd.founded_at,
                now: today,
            });
        }

        Ok(EntityEvent::Registered(EntityRegisteredV1 {
            entity_id: cmd.entity_id,
            canonical_name: cmd.canonical_name,
            entity_type: cmd.entity_type,
            jurisdiction: cmd.jurisdiction,
            registration_number_in_jurisdiction: cmd.registration_number_in_jurisdiction,
            founded_at: cmd.founded_at,
            registered_by_principal: cmd.registered_by_principal,
            registered_at: cmd.registered_at,
            correlation_id: cmd.correlation_id,
        }))
    }

    /// Handle an Update command. Refuses if the aggregate doesn't yet
    /// exist or has been dissolved. Emits `EntityUpdatedV1` with
    /// before/after snapshots.
    pub fn handle_update(&self, cmd: UpdateEntity) -> Result<EntityEvent, DomainError> {
        let snap = match (self.state.clone(), self.snapshot.as_ref()) {
            (EntityState::Absent, _) | (_, None) => {
                return Err(DomainError::UpdateBeforeRegistration(self.id.0));
            }
            (EntityState::Dissolved, _) => {
                return Err(DomainError::UpdateOnDissolvedEntity(self.id.0));
            }
            (EntityState::Active, Some(s)) => s,
        };
        let before = UpdatableFields {
            canonical_name: snap.canonical_name.clone(),
            entity_type: snap.entity_type.clone(),
        };
        Ok(EntityEvent::Updated(EntityUpdatedV1 {
            entity_id: self.id,
            before,
            after: cmd.after,
            updated_by_principal: cmd.updated_by_principal,
            updated_at: cmd.updated_at,
            correlation_id: cmd.correlation_id,
        }))
    }

    /// Handle a Dissolve command. Refuses if the aggregate doesn't
    /// exist, if the dissolution date precedes foundation, or if the
    /// entity is already dissolved on a DIFFERENT date. A repeat
    /// dissolution on the SAME date is treated as idempotent at the
    /// use-case layer (not here).
    pub fn handle_dissolve(&self, cmd: DissolveEntity) -> Result<EntityEvent, DomainError> {
        let snap = match (self.state.clone(), self.snapshot.as_ref()) {
            (EntityState::Absent, _) | (_, None) => {
                return Err(DomainError::DissolveBeforeRegistration(self.id.0));
            }
            (EntityState::Dissolved, Some(s)) => {
                let existing = s.dissolved_at.expect(
                    "dissolved state implies dissolved_at is set; aggregate invariant",
                );
                return Err(DomainError::AlreadyDissolved {
                    entity_id: self.id.0,
                    dissolved_at: existing,
                });
            }
            (EntityState::Active, Some(s)) => s,
        };
        if cmd.dissolved_at < snap.founded_at {
            return Err(DomainError::DissolutionBeforeFoundation {
                entity_id: self.id.0,
                founded_at: snap.founded_at,
                dissolved_at: cmd.dissolved_at,
            });
        }
        Ok(EntityEvent::Dissolved(EntityDissolvedV1 {
            entity_id: self.id,
            dissolved_at: cmd.dissolved_at,
            dissolved_by_principal: cmd.dissolved_by_principal,
            recorded_at: cmd.recorded_at,
            correlation_id: cmd.correlation_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};
    use uuid::Uuid;

    use super::*;

    fn id() -> EntityId {
        EntityId(Uuid::now_v7())
    }

    fn register_cmd(id: EntityId) -> RegisterEntity {
        RegisterEntity {
            entity_id: id,
            canonical_name: CanonicalName::try_from_str("ACME SARL").unwrap(),
            entity_type: EntityType::Sarl,
            jurisdiction: Jurisdiction::try_from_str("CM").unwrap(),
            registration_number_in_jurisdiction: RegistrationNumber::try_from_str(
                "RC/DLA/2024/B/12345",
            )
            .unwrap(),
            founded_at: date!(2024 - 01 - 15),
            registered_by_principal: "spiffe://recor.cm/admin-1".into(),
            registered_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn now() -> OffsetDateTime {
        datetime!(2026-05-12 12:00:00 UTC)
    }

    #[test]
    fn register_on_absent_emits_registered_event() {
        let id = id();
        let agg = EntityAggregate::from_events(id, &[]);
        let event = agg.handle_register(register_cmd(id), now()).unwrap();
        assert!(matches!(event, EntityEvent::Registered(_)));
        assert_eq!(event.entity_id(), id);
    }

    #[test]
    fn register_on_active_rejects() {
        let id = id();
        let agg = EntityAggregate::from_events(id, &[]);
        let event = agg.handle_register(register_cmd(id), now()).unwrap();
        let mut agg = agg;
        agg.apply(&event);
        let err = agg
            .handle_register(register_cmd(id), now())
            .expect_err("duplicate register must reject");
        assert!(matches!(err, DomainError::AlreadyRegistered(_)));
    }

    #[test]
    fn register_with_future_founded_at_rejects() {
        let id = id();
        let agg = EntityAggregate::from_events(id, &[]);
        let mut cmd = register_cmd(id);
        cmd.founded_at = date!(2099 - 12 - 31);
        let err = agg.handle_register(cmd, now()).expect_err("future date refused");
        assert!(matches!(err, DomainError::FoundedAtInFuture { .. }));
    }

    #[test]
    fn update_records_before_and_after_snapshots() {
        let id = id();
        let mut agg = EntityAggregate::from_events(id, &[]);
        let registered = agg.handle_register(register_cmd(id), now()).unwrap();
        agg.apply(&registered);

        let update_cmd = UpdateEntity {
            entity_id: id,
            after: UpdatableFields {
                canonical_name: CanonicalName::try_from_str("ACME Mining SARL").unwrap(),
                entity_type: EntityType::Sarl,
            },
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let event = agg.handle_update(update_cmd).unwrap();
        let EntityEvent::Updated(p) = &event else {
            panic!("expected Updated event")
        };
        assert_eq!(p.before.canonical_name.as_str(), "ACME SARL");
        assert_eq!(p.after.canonical_name.as_str(), "ACME Mining SARL");
    }

    #[test]
    fn update_on_dissolved_rejects() {
        let id = id();
        let mut agg = EntityAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let dissolve = DissolveEntity {
            entity_id: id,
            dissolved_at: date!(2026 - 04 - 01),
            dissolved_by_principal: "spiffe://recor.cm/admin-1".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        agg.apply(&agg.handle_dissolve(dissolve).unwrap());

        let update_cmd = UpdateEntity {
            entity_id: id,
            after: UpdatableFields {
                canonical_name: CanonicalName::try_from_str("New name").unwrap(),
                entity_type: EntityType::Sarl,
            },
            updated_by_principal: "x".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg.handle_update(update_cmd).expect_err("update on dissolved refused");
        assert!(matches!(err, DomainError::UpdateOnDissolvedEntity(_)));
    }

    #[test]
    fn dissolve_before_foundation_rejects() {
        let id = id();
        let mut agg = EntityAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let dissolve = DissolveEntity {
            entity_id: id,
            // founded_at == 2024-01-15; pick an earlier date
            dissolved_at: date!(2020 - 01 - 01),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg.handle_dissolve(dissolve).expect_err("invalid date refused");
        assert!(matches!(err, DomainError::DissolutionBeforeFoundation { .. }));
    }

    #[test]
    fn dissolve_on_already_dissolved_rejects() {
        let id = id();
        let mut agg = EntityAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let first = DissolveEntity {
            entity_id: id,
            dissolved_at: date!(2026 - 04 - 01),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        agg.apply(&agg.handle_dissolve(first.clone()).unwrap());

        let again = DissolveEntity {
            entity_id: id,
            dissolved_at: date!(2026 - 04 - 02),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg.handle_dissolve(again).expect_err("re-dissolve refused");
        assert!(matches!(err, DomainError::AlreadyDissolved { .. }));
    }

    #[test]
    fn update_on_absent_rejects() {
        let id = id();
        let agg = EntityAggregate::from_events(id, &[]);
        let update_cmd = UpdateEntity {
            entity_id: id,
            after: UpdatableFields {
                canonical_name: CanonicalName::try_from_str("never registered").unwrap(),
                entity_type: EntityType::Sarl,
            },
            updated_by_principal: "x".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg.handle_update(update_cmd).expect_err("update on absent refused");
        assert!(matches!(err, DomainError::UpdateBeforeRegistration(_)));
    }
}
