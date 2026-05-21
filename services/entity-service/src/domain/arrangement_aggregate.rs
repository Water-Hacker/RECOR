//! Arrangement aggregate. Event-sourced; commands produce events; events
//! are folded to derive the current state.
//!
//! Lifecycle: `Absent` → `Active` (Registered) → `Dissolved`.
//! Updates are permitted only while `Active`.
//!
//! Pure: no I/O, no logging, no async. The aggregate owns its R.25
//! invariants and surfaces violations as `ArrangementDomainError`.

use time::{Date, Duration, OffsetDateTime};

use super::arrangement_command::{
    DissolveArrangement, RegisterArrangement, UpdateArrangement,
};
use super::arrangement_error::ArrangementDomainError;
use super::arrangement_event::{
    ArrangementDissolvedV1, ArrangementEvent, ArrangementRegisteredV1, ArrangementUpdatedV1,
};
use super::arrangement_value_object::{
    ArrangementId, ArrangementKind, ArrangementUpdatableFields, GoverningLawJurisdiction,
};

/// FATF R.25 INR §3.f — five-year-after-cessation retention period.
/// Multiplied by 365 + 1 day so a leap year inside the window never
/// shaves the deadline below five calendar years. (Using `366 * 5` is
/// a deliberate conservative ceiling; the back-office may extend.)
const RETENTION_DAYS_AFTER_DISSOLUTION: i64 = 366 * 5;

/// In-memory aggregate state — the materialised result of folding the
/// event log. The Postgres projection mirrors this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrangementAggregate {
    pub id: ArrangementId,
    pub state: ArrangementState,
    pub version: u64,
    pub snapshot: Option<ArrangementSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrangementState {
    Absent,
    Active,
    Dissolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrangementSnapshot {
    pub arrangement_kind: ArrangementKind,
    pub governing_law_jurisdiction: GoverningLawJurisdiction,
    pub constitution_date: Date,
    pub dissolution_date: Option<Date>,
    pub retention_until: Option<Date>,
    pub fields: ArrangementUpdatableFields,
}

impl ArrangementAggregate {
    /// Build the aggregate by replaying its event log. An empty log
    /// yields the `Absent` state at version 0.
    pub fn from_events(id: ArrangementId, events: &[ArrangementEvent]) -> Self {
        let mut agg = Self {
            id,
            state: ArrangementState::Absent,
            version: 0,
            snapshot: None,
        };
        for e in events {
            agg.apply(e);
        }
        agg
    }

    /// Fold an event into the in-memory state. Always increments version.
    pub fn apply(&mut self, event: &ArrangementEvent) {
        if event.arrangement_id() != self.id {
            return;
        }
        match event {
            ArrangementEvent::Registered(p) => {
                self.state = ArrangementState::Active;
                self.snapshot = Some(ArrangementSnapshot {
                    arrangement_kind: p.arrangement_kind,
                    governing_law_jurisdiction: p.governing_law_jurisdiction.clone(),
                    constitution_date: p.constitution_date,
                    dissolution_date: None,
                    retention_until: None,
                    fields: p.fields.clone(),
                });
            }
            ArrangementEvent::Updated(p) => {
                if let Some(snap) = self.snapshot.as_mut() {
                    snap.fields = p.after.clone();
                }
            }
            ArrangementEvent::Dissolved(p) => {
                self.state = ArrangementState::Dissolved;
                if let Some(snap) = self.snapshot.as_mut() {
                    snap.dissolution_date = Some(p.dissolution_date);
                    snap.retention_until = Some(p.retention_until);
                }
            }
        }
        self.version = self.version.saturating_add(1);
    }

    /// Handle a Register command. Refuses if the arrangement already
    /// exists, if any aggregate invariant is violated, or if a value-
    /// object boundary check fails.
    pub fn handle_register(
        &self,
        cmd: RegisterArrangement,
        now_utc: OffsetDateTime,
    ) -> Result<ArrangementEvent, ArrangementDomainError> {
        if !matches!(self.state, ArrangementState::Absent) {
            return Err(ArrangementDomainError::AlreadyRegistered(self.id.0));
        }
        if cmd.arrangement_id != self.id {
            return Err(ArrangementDomainError::AlreadyRegistered(self.id.0));
        }
        // R.25 invariants: at least one settlor + at least one trustee.
        if cmd.fields.settlor_refs.is_empty() {
            return Err(ArrangementDomainError::NoSettlor);
        }
        if cmd.fields.trustee_refs.is_empty() {
            return Err(ArrangementDomainError::NoTrustee);
        }
        // Per-entry validation (TrusteeRef discriminator etc.).
        cmd.fields.validate()?;
        for t in &cmd.fields.trustee_refs {
            t.validate()?;
        }
        // Temporal invariant: cannot constitute an arrangement in the future.
        let today = now_utc.date();
        if cmd.constitution_date > today {
            return Err(ArrangementDomainError::ConstitutionDateInFuture {
                constitution_date: cmd.constitution_date,
                now: today,
            });
        }

        Ok(ArrangementEvent::Registered(ArrangementRegisteredV1 {
            arrangement_id: cmd.arrangement_id,
            arrangement_kind: cmd.arrangement_kind,
            governing_law_jurisdiction: cmd.governing_law_jurisdiction,
            constitution_date: cmd.constitution_date,
            fields: cmd.fields,
            registered_by_principal: cmd.registered_by_principal,
            registered_at: cmd.registered_at,
            correlation_id: cmd.correlation_id,
        }))
    }

    /// Handle an Update command. Refuses if the arrangement does not
    /// exist or is already dissolved.
    pub fn handle_update(
        &self,
        cmd: UpdateArrangement,
    ) -> Result<ArrangementEvent, ArrangementDomainError> {
        let snap = match (self.state.clone(), self.snapshot.as_ref()) {
            (ArrangementState::Absent, _) | (_, None) => {
                return Err(ArrangementDomainError::UpdateBeforeRegistration(self.id.0));
            }
            (ArrangementState::Dissolved, _) => {
                return Err(ArrangementDomainError::UpdateOnDissolved(self.id.0));
            }
            (ArrangementState::Active, Some(s)) => s,
        };
        // R.25 invariants are preserved across updates.
        if cmd.after.settlor_refs.is_empty() {
            return Err(ArrangementDomainError::NoSettlor);
        }
        if cmd.after.trustee_refs.is_empty() {
            return Err(ArrangementDomainError::NoTrustee);
        }
        cmd.after.validate()?;

        Ok(ArrangementEvent::Updated(ArrangementUpdatedV1 {
            arrangement_id: self.id,
            before: snap.fields.clone(),
            after: cmd.after,
            updated_by_principal: cmd.updated_by_principal,
            updated_at: cmd.updated_at,
            correlation_id: cmd.correlation_id,
        }))
    }

    /// Handle a Dissolve command. Refuses if the arrangement does not
    /// exist, is already dissolved, or if the dissolution date is not
    /// strictly after the constitution date. Automatically computes the
    /// R.25 INR §3.f 5-year retention deadline.
    pub fn handle_dissolve(
        &self,
        cmd: DissolveArrangement,
    ) -> Result<ArrangementEvent, ArrangementDomainError> {
        let snap = match (self.state.clone(), self.snapshot.as_ref()) {
            (ArrangementState::Absent, _) | (_, None) => {
                return Err(ArrangementDomainError::DissolveBeforeRegistration(self.id.0));
            }
            (ArrangementState::Dissolved, Some(s)) => {
                let existing = s.dissolution_date.expect(
                    "dissolved state implies dissolution_date set — aggregate invariant",
                );
                return Err(ArrangementDomainError::AlreadyDissolved {
                    arrangement_id: self.id.0,
                    dissolution_date: existing,
                });
            }
            (ArrangementState::Active, Some(s)) => s,
        };
        if cmd.dissolution_date <= snap.constitution_date {
            return Err(ArrangementDomainError::DissolutionBeforeOrEqualConstitution {
                constitution_date: snap.constitution_date,
                dissolution_date: cmd.dissolution_date,
            });
        }
        let retention_until = cmd
            .dissolution_date
            .checked_add(Duration::days(RETENTION_DAYS_AFTER_DISSOLUTION))
            .ok_or(ArrangementDomainError::RetentionUntilOverflow(
                cmd.dissolution_date,
            ))?;

        Ok(ArrangementEvent::Dissolved(ArrangementDissolvedV1 {
            arrangement_id: self.id,
            dissolution_date: cmd.dissolution_date,
            retention_until,
            dissolved_by_principal: cmd.dissolved_by_principal,
            recorded_at: cmd.recorded_at,
            correlation_id: cmd.correlation_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::arrangement_value_object::{
        ControlExerciseRef, SettlorRef, TrusteeRef,
    };
    use super::*;
    use time::macros::{date, datetime};
    use uuid::Uuid;

    fn id() -> ArrangementId {
        ArrangementId(Uuid::now_v7())
    }

    fn now() -> OffsetDateTime {
        datetime!(2026-05-12 12:00:00 UTC)
    }

    fn fields_with_one_settlor_and_one_trustee() -> ArrangementUpdatableFields {
        ArrangementUpdatableFields {
            settlor_refs: vec![SettlorRef {
                person_id: Uuid::now_v7(),
                role_metadata: None,
            }],
            trustee_refs: vec![TrusteeRef {
                person_id: Some(Uuid::now_v7()),
                entity_id: None,
                fiduciary_registration_id: None,
                role_metadata: None,
            }],
            protector_refs: vec![],
            named_beneficiary_refs: vec![],
            class_beneficiary_specs: vec![],
            control_exercise_refs: vec![],
        }
    }

    fn register_cmd(id: ArrangementId) -> RegisterArrangement {
        RegisterArrangement {
            arrangement_id: id,
            arrangement_kind: ArrangementKind::ExpressTrust,
            governing_law_jurisdiction: GoverningLawJurisdiction::try_from_str("CM").unwrap(),
            constitution_date: date!(2024 - 06 - 01),
            fields: fields_with_one_settlor_and_one_trustee(),
            registered_by_principal: "spiffe://recor.cm/admin-1".into(),
            registered_at: datetime!(2026-05-01 10:00:00 UTC),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn register_on_absent_emits_event() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let ev = agg.handle_register(register_cmd(id), now()).unwrap();
        assert!(matches!(ev, ArrangementEvent::Registered(_)));
        assert_eq!(ev.arrangement_id(), id);
    }

    #[test]
    fn register_on_active_rejects_as_duplicate() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        let ev = agg.handle_register(register_cmd(id), now()).unwrap();
        agg.apply(&ev);
        let err = agg
            .handle_register(register_cmd(id), now())
            .expect_err("duplicate register refused");
        assert!(matches!(err, ArrangementDomainError::AlreadyRegistered(_)));
    }

    #[test]
    fn register_with_future_constitution_date_rejects() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let mut cmd = register_cmd(id);
        cmd.constitution_date = date!(2099 - 01 - 01);
        let err = agg
            .handle_register(cmd, now())
            .expect_err("future date refused");
        assert!(matches!(
            err,
            ArrangementDomainError::ConstitutionDateInFuture { .. }
        ));
    }

    #[test]
    fn register_without_settlor_refuses() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let mut cmd = register_cmd(id);
        cmd.fields.settlor_refs.clear();
        let err = agg
            .handle_register(cmd, now())
            .expect_err("R.25 §3.a refuses missing settlor");
        assert!(matches!(err, ArrangementDomainError::NoSettlor));
    }

    #[test]
    fn register_without_trustee_refuses() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let mut cmd = register_cmd(id);
        cmd.fields.trustee_refs.clear();
        let err = agg
            .handle_register(cmd, now())
            .expect_err("R.25 §3.b refuses missing trustee");
        assert!(matches!(err, ArrangementDomainError::NoTrustee));
    }

    #[test]
    fn register_with_malformed_trustee_ref_refuses() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let mut cmd = register_cmd(id);
        cmd.fields.trustee_refs = vec![TrusteeRef {
            person_id: Some(Uuid::now_v7()),
            entity_id: Some(Uuid::now_v7()), // two discriminators
            fiduciary_registration_id: None,
            role_metadata: None,
        }];
        let err = agg
            .handle_register(cmd, now())
            .expect_err("malformed trustee ref refused");
        assert!(matches!(
            err,
            ArrangementDomainError::ValueObject(
                super::super::arrangement_value_object::ArrangementValueObjectError::TrusteeRefShape(_)
            )
        ));
    }

    #[test]
    fn update_records_before_and_after_snapshots() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        let registered = agg.handle_register(register_cmd(id), now()).unwrap();
        agg.apply(&registered);

        let mut after = fields_with_one_settlor_and_one_trustee();
        after.control_exercise_refs.push(ControlExerciseRef {
            person_id: Uuid::now_v7(),
            control_basis: "Settlor-puppet trustee identified by tax filings".into(),
        });

        let upd = UpdateArrangement {
            arrangement_id: id,
            after: after.clone(),
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let ev = agg.handle_update(upd).unwrap();
        let ArrangementEvent::Updated(p) = &ev else {
            panic!("expected Updated event");
        };
        assert!(p.before.control_exercise_refs.is_empty());
        assert_eq!(p.after.control_exercise_refs.len(), 1);
    }

    #[test]
    fn update_on_dissolved_rejects() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());

        let diss = DissolveArrangement {
            arrangement_id: id,
            dissolution_date: date!(2026 - 04 - 01),
            dissolved_by_principal: "spiffe://recor.cm/admin-1".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        agg.apply(&agg.handle_dissolve(diss).unwrap());

        let upd = UpdateArrangement {
            arrangement_id: id,
            after: fields_with_one_settlor_and_one_trustee(),
            updated_by_principal: "spiffe://recor.cm/admin-1".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg
            .handle_update(upd)
            .expect_err("update on dissolved refused");
        assert!(matches!(err, ArrangementDomainError::UpdateOnDissolved(_)));
    }

    #[test]
    fn update_on_absent_rejects() {
        let id = id();
        let agg = ArrangementAggregate::from_events(id, &[]);
        let upd = UpdateArrangement {
            arrangement_id: id,
            after: fields_with_one_settlor_and_one_trustee(),
            updated_by_principal: "x".into(),
            updated_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg
            .handle_update(upd)
            .expect_err("update on absent refused");
        assert!(matches!(
            err,
            ArrangementDomainError::UpdateBeforeRegistration(_)
        ));
    }

    #[test]
    fn dissolve_before_constitution_rejects() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let diss = DissolveArrangement {
            arrangement_id: id,
            // constitution_date == 2024-06-01
            dissolution_date: date!(2024 - 06 - 01),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg
            .handle_dissolve(diss)
            .expect_err("dissolution date ≤ constitution refused");
        assert!(matches!(
            err,
            ArrangementDomainError::DissolutionBeforeOrEqualConstitution { .. }
        ));
    }

    #[test]
    fn dissolve_sets_retention_until_to_constitution_plus_five_years() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let dissolution_date = date!(2026 - 04 - 01);
        let diss = DissolveArrangement {
            arrangement_id: id,
            dissolution_date,
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let ev = agg.handle_dissolve(diss).unwrap();
        let ArrangementEvent::Dissolved(p) = ev else {
            panic!("expected Dissolved event");
        };
        // retention_until must be ≥ 5 calendar years after dissolution;
        // we use 366*5 days as the conservative ceiling, so the year
        // delta is exactly 5 (plus or minus one day on a leap-year edge).
        let delta = (p.retention_until - p.dissolution_date).whole_days();
        assert!(
            (1825..=1830).contains(&delta),
            "5-year retention window expected (1825..=1830 days), got {delta}"
        );
    }

    #[test]
    fn dissolve_on_dissolved_rejects() {
        let id = id();
        let mut agg = ArrangementAggregate::from_events(id, &[]);
        agg.apply(&agg.handle_register(register_cmd(id), now()).unwrap());
        let first = DissolveArrangement {
            arrangement_id: id,
            dissolution_date: date!(2026 - 04 - 01),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        agg.apply(&agg.handle_dissolve(first.clone()).unwrap());

        let again = DissolveArrangement {
            arrangement_id: id,
            dissolution_date: date!(2026 - 04 - 02),
            dissolved_by_principal: "x".into(),
            recorded_at: now(),
            correlation_id: Uuid::now_v7(),
        };
        let err = agg
            .handle_dissolve(again)
            .expect_err("re-dissolve refused");
        assert!(matches!(err, ArrangementDomainError::AlreadyDissolved { .. }));
    }

    #[test]
    fn apply_silently_drops_events_for_wrong_aggregate() {
        let id_a = id();
        let id_b = id();
        let mut agg = ArrangementAggregate::from_events(id_a, &[]);
        let ev_b = ArrangementAggregate::from_events(id_b, &[])
            .handle_register(register_cmd(id_b), now())
            .unwrap();
        let version_before = agg.version;
        agg.apply(&ev_b);
        assert_eq!(agg.version, version_before, "wrong-id event must not advance version");
        assert_eq!(agg.state, ArrangementState::Absent);
    }
}
