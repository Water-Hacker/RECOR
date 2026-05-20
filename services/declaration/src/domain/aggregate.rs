//! Declaration aggregate. Event-sourced.
//!
//! `DeclarationAggregate` is the unit of consistency. Commands are
//! validated against current aggregate state; valid commands produce
//! events; events are applied to update state.
//!
//! Invariants enforced here:
//!   - At least one beneficial owner per declaration
//!   - Sum of ownership basis points across owners equals 10_000 (100%)
//!     for the first version of the aggregate (a future Amend command
//!     may relax this to allow partial-ownership declarations)
//!   - No duplicate person_id within a single declaration
//!   - effective_from is in the past 5 years and not after submitted_at
//!   - declaration_id may receive a Submit command only once
//!   - The attestation's signed_by matches the command's declarant_principal

use std::collections::HashSet;

use blake3::Hasher;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use super::command::{
    AmendDeclaration, CorrectDeclaration, RecordVerificationOutcome, SubmitDeclaration,
};
use super::error::DomainError;
use super::event::{
    DeclarationAmendedV1, DeclarationCorrectedV1, DeclarationEvent, DeclarationSubmittedV1,
    DeclarationSupersededV1, DeclarationVerifiedV1,
};
use super::value_object::{
    AmendmentSet, BeneficialOwnerClaim, BoCascadeTier, CorrectionSet, DeclarationId,
    DeclarationState,
};

/// In-memory representation of a Declaration aggregate, hydrated from
/// its event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationAggregate {
    pub id: DeclarationId,
    /// Monotonic event count, used for optimistic concurrency.
    pub version: u64,
    pub state: DeclarationState,
    /// The verification case that produced the current state, if any.
    /// Set when a `declaration.verified.v1` event is applied; replays
    /// of the same case_id against the same aggregate are no-ops.
    pub verification_case_id: Option<uuid::Uuid>,
    /// The declaration that has superseded THIS one, if any. Set when
    /// a `declaration.superseded.v1` event is applied. A declaration
    /// can be superseded at most once (idempotency anchor — supersede
    /// chains are strictly linear).
    pub superseded_by: Option<DeclarationId>,
    /// The declarant principal recorded on the Submitted event. Used
    /// by the use-case layer to authorise subsequent commands (Supersede,
    /// Amend, Correct).
    pub declarant_principal: Option<String>,
    /// The entity this declaration is about. Used to validate that
    /// superseding declarations target the same entity.
    pub entity_id: Option<super::value_object::EntityId>,
    /// Current snapshot of the amendable fields. Populated from the
    /// Submitted event and replaced by subsequent Amended events.
    /// Used by `handle_amend` to populate the `before` snapshot.
    pub amendment_state: Option<AmendmentSet>,
    /// Current snapshot of the correctable fields. Always populated
    /// (empty after Submitted; updated by Corrected events). Used by
    /// `handle_correct` to populate the `before` snapshot.
    pub correction_state: CorrectionSet,
}

impl DeclarationAggregate {
    /// Construct a fresh aggregate at version 0, no events applied yet.
    pub fn fresh(id: DeclarationId) -> Self {
        Self {
            id,
            version: 0,
            state: DeclarationState::Draft, // aggregate-not-yet-emitting; "Draft" is the absent placeholder
            verification_case_id: None,
            superseded_by: None,
            declarant_principal: None,
            entity_id: None,
            amendment_state: None,
            correction_state: CorrectionSet::default(),
        }
    }

    /// Hydrate by replaying events in order.
    pub fn from_events(id: DeclarationId, events: &[DeclarationEvent]) -> Self {
        let mut agg = Self::fresh(id);
        for event in events {
            agg.apply(event);
        }
        agg
    }

    /// Apply an event to advance state. Pure; no I/O.
    pub fn apply(&mut self, event: &DeclarationEvent) {
        match event {
            DeclarationEvent::Submitted(p) => {
                self.state = DeclarationState::Submitted;
                self.declarant_principal = Some(p.declarant_principal.clone());
                self.entity_id = Some(p.entity_id);
                self.amendment_state = Some(AmendmentSet {
                    beneficial_owners: p.beneficial_owners.clone(),
                    effective_from: p.effective_from,
                    declarant_role: p.declarant_role,
            adequacy_claims: None,
                });
                // Correction state starts empty; Corrected events
                // overwrite it.
                self.correction_state = CorrectionSet::default();
            }
            DeclarationEvent::Verified(p) => {
                self.state = p.lane.to_declaration_state();
                self.verification_case_id = Some(p.verification_case_id);
            }
            DeclarationEvent::Superseded(p) => {
                self.state = DeclarationState::Superseded;
                self.superseded_by = Some(p.superseded_by_declaration_id);
            }
            DeclarationEvent::Amended(p) => {
                // The aggregate's lifecycle state is unchanged by an
                // amendment (still Submitted or InVerification). The
                // amendable-field snapshot is replaced wholesale by
                // `after`.
                self.amendment_state = Some(p.after.clone());
            }
            DeclarationEvent::Corrected(p) => {
                // Corrections only touch metadata; the lifecycle state
                // stays Submitted. Replace the snapshot wholesale.
                self.correction_state = p.after.clone();
            }
        }
        self.version = self.version.saturating_add(1);
    }

    /// Validate a Submit command and produce the resulting event.
    /// Does NOT mutate `self`; the caller decides whether to apply
    /// after persistence succeeds.
    pub fn handle_submit(
        &self,
        cmd: SubmitDeclaration,
    ) -> Result<DeclarationEvent, DomainError> {
        if self.version > 0 {
            return Err(DomainError::AlreadySubmitted(self.id.0));
        }
        validate_command(&cmd)?;

        let receipt_hash_hex = compute_receipt_hash(&cmd);

        let payload = DeclarationSubmittedV1 {
            declaration_id: cmd.declaration_id,
            entity_id: cmd.entity_id,
            declarant_principal: cmd.declarant_principal,
            declarant_role: cmd.declarant_role,
            kind: cmd.kind,
            effective_from: cmd.effective_from,
            beneficial_owners: cmd.beneficial_owners,
            attestation: cmd.attestation,
            adequacy_claims: cmd.adequacy_claims,
            last_event_observed_at: cmd.last_event_observed_at,
            submitted_at: cmd.submitted_at,
            correlation_id: cmd.correlation_id,
            receipt_hash_hex,
        };

        Ok(DeclarationEvent::Submitted(payload))
    }

    /// Validate a RecordVerificationOutcome command and produce the
    /// resulting event. Idempotent at the boundary: if the same case_id
    /// has already been applied, returns `None` so the caller skips the
    /// write. A different case_id against an already-verified aggregate
    /// is a domain error — the writeback channel must not re-verify a
    /// declaration without explicit re-verification semantics, which v1
    /// does not support.
    pub fn handle_record_verification(
        &self,
        cmd: RecordVerificationOutcome,
    ) -> Result<Option<DeclarationEvent>, DomainError> {
        if self.version == 0 {
            return Err(DomainError::VerificationOutcomeBeforeSubmit(self.id.0));
        }

        validate_belief("fused_authenticity_belief", cmd.fused_authenticity_belief)?;
        validate_belief(
            "fused_authenticity_plausibility",
            cmd.fused_authenticity_plausibility,
        )?;
        validate_belief("fused_risk_belief", cmd.fused_risk_belief)?;

        if let Some(existing) = self.verification_case_id {
            return if existing == cmd.verification_case_id {
                // Replay of the same case — caller's writeback delivered twice.
                // No new event; the aggregate is already at the post-verification
                // state. Use case treats `None` as "ack and dispatch outbox".
                Ok(None)
            } else {
                Err(DomainError::VerificationCaseMismatch {
                    declaration_id: self.id.0,
                    existing_case_id: existing,
                    new_case_id: cmd.verification_case_id,
                })
            };
        }

        let payload = DeclarationVerifiedV1 {
            declaration_id: cmd.declaration_id,
            verification_case_id: cmd.verification_case_id,
            lane: cmd.lane,
            fused_authenticity_belief: cmd.fused_authenticity_belief,
            fused_authenticity_plausibility: cmd.fused_authenticity_plausibility,
            fused_risk_belief: cmd.fused_risk_belief,
            completed_at: cmd.completed_at,
            recorded_at: OffsetDateTime::now_utc(),
        };
        Ok(Some(DeclarationEvent::Verified(payload)))
    }

    /// Validate a Supersede command against THIS aggregate (the OLD
    /// declaration being replaced). Produces the
    /// `DeclarationSupersededV1` event; the caller is responsible for
    /// persisting it atomically alongside the NEW declaration's
    /// `DeclarationSubmittedV1`.
    ///
    /// Rules enforced here:
    ///   - This aggregate must have a prior Submitted event (version > 0)
    ///   - This aggregate must NOT already be superseded (chains are linear)
    ///   - The current state must be `Accepted` or `InVerification` —
    ///     Draft/Submitted-not-yet-verified should be re-submitted
    ///     instead; Rejected declarations cannot supersede anything
    ///     because they aren't authoritative
    ///   - The successor must not be the same id as this aggregate
    ///     (self-supersede is meaningless)
    pub fn handle_supersede(
        &self,
        superseded_by_declaration_id: DeclarationId,
        correlation_id: uuid::Uuid,
    ) -> Result<DeclarationEvent, DomainError> {
        if self.version == 0 {
            return Err(DomainError::SupersedeBeforeSubmit(self.id.0));
        }
        if self.superseded_by.is_some() {
            return Err(DomainError::AlreadySuperseded(self.id.0));
        }
        if superseded_by_declaration_id == self.id {
            return Err(DomainError::SelfSupersedeForbidden(self.id.0));
        }
        match self.state {
            DeclarationState::Accepted | DeclarationState::InVerification => {}
            _ => {
                return Err(DomainError::SupersedeFromInvalidState {
                    declaration_id: self.id.0,
                    state: self.state.as_str(),
                });
            }
        }

        let payload = DeclarationSupersededV1 {
            declaration_id: self.id,
            superseded_by_declaration_id,
            superseded_at: OffsetDateTime::now_utc(),
            correlation_id,
        };
        Ok(DeclarationEvent::Superseded(payload))
    }

    /// Validate an Amend command and produce a `DeclarationAmendedV1`
    /// event. Does NOT mutate `self`; the caller persists then applies.
    ///
    /// Rules enforced here:
    ///   - This aggregate must have a prior Submitted event (version > 0).
    ///   - This aggregate must NOT be Superseded.
    ///   - Current state must be `Submitted` or `InVerification`.
    ///     Accepted → operators must use Supersede (more transparency);
    ///     Rejected → re-submission.
    ///   - The command's `declarant_principal` must match the principal
    ///     stored on the aggregate (only the owner can amend).
    ///   - The attestation's `signed_by` must match the command principal.
    ///   - `amendments.beneficial_owners` must satisfy the same invariants
    ///     as a fresh submission (non-empty, no duplicate person_id,
    ///     sum to 10_000 basis points).
    ///   - `amendments.effective_from` must be in the past 5 years and
    ///     not after `submitted_at`.
    pub fn handle_amend(
        &self,
        cmd: AmendDeclaration,
    ) -> Result<DeclarationEvent, DomainError> {
        if self.version == 0 {
            return Err(DomainError::AmendBeforeSubmit(self.id.0));
        }
        if self.superseded_by.is_some() {
            return Err(DomainError::AmendFromInvalidState {
                declaration_id: self.id.0,
                state: DeclarationState::Superseded.as_str(),
            });
        }
        match self.state {
            DeclarationState::Submitted | DeclarationState::InVerification => {}
            _ => {
                return Err(DomainError::AmendFromInvalidState {
                    declaration_id: self.id.0,
                    state: self.state.as_str(),
                });
            }
        }

        // Authorisation: only the original declarant can amend.
        let expected_owner = self.declarant_principal.clone().ok_or_else(|| {
            DomainError::AmendBeforeSubmit(self.id.0)
        })?;
        if expected_owner != cmd.declarant_principal {
            return Err(DomainError::AmendNotOwner {
                declaration_id: self.id.0,
                expected: expected_owner,
                actual: cmd.declarant_principal,
            });
        }
        if cmd.attestation.signed_by != cmd.declarant_principal {
            return Err(DomainError::AttestationPrincipalMismatch {
                expected: cmd.declarant_principal,
                actual: cmd.attestation.signed_by,
            });
        }

        // Validate the amended payload against the same invariants as
        // a fresh submission (owners non-empty, sum to 10_000, no
        // duplicate person_id, effective_from in window).
        validate_beneficial_owners(&cmd.amendments.beneficial_owners)?;
        validate_effective_from(cmd.amendments.effective_from, cmd.submitted_at)?;

        // The `before` snapshot is the aggregate's current amendable
        // state; populated when the Submitted event was applied (and
        // updated by any intervening Amended events).
        let before = self.amendment_state.clone().ok_or_else(|| {
            DomainError::AmendBeforeSubmit(self.id.0)
        })?;

        let payload = DeclarationAmendedV1 {
            declaration_id: self.id,
            before,
            after: cmd.amendments,
            attestation: cmd.attestation,
            amended_at: OffsetDateTime::now_utc(),
            correlation_id: cmd.correlation_id,
        };
        Ok(DeclarationEvent::Amended(payload))
    }

    /// Validate a Correct command and produce a `DeclarationCorrectedV1`
    /// event. Stricter than amend — corrections are admissible only
    /// from `Submitted` (pre-verification).
    pub fn handle_correct(
        &self,
        cmd: CorrectDeclaration,
    ) -> Result<DeclarationEvent, DomainError> {
        if self.version == 0 {
            return Err(DomainError::CorrectBeforeSubmit(self.id.0));
        }
        if self.superseded_by.is_some() {
            return Err(DomainError::CorrectFromInvalidState {
                declaration_id: self.id.0,
                state: DeclarationState::Superseded.as_str(),
            });
        }
        if self.state != DeclarationState::Submitted {
            return Err(DomainError::CorrectFromInvalidState {
                declaration_id: self.id.0,
                state: self.state.as_str(),
            });
        }

        let expected_owner = self.declarant_principal.clone().ok_or_else(|| {
            DomainError::CorrectBeforeSubmit(self.id.0)
        })?;
        if expected_owner != cmd.declarant_principal {
            return Err(DomainError::CorrectNotOwner {
                declaration_id: self.id.0,
                expected: expected_owner,
                actual: cmd.declarant_principal,
            });
        }
        if cmd.attestation.signed_by != cmd.declarant_principal {
            return Err(DomainError::AttestationPrincipalMismatch {
                expected: cmd.declarant_principal,
                actual: cmd.attestation.signed_by,
            });
        }

        let before = self.correction_state.clone();
        // Normalise empty strings to None at the boundary so the
        // event log carries canonical shape.
        let after = normalise_correction_set(cmd.corrections);

        let payload = DeclarationCorrectedV1 {
            declaration_id: self.id,
            before,
            after,
            attestation: cmd.attestation,
            corrected_at: OffsetDateTime::now_utc(),
            correlation_id: cmd.correlation_id,
        };
        Ok(DeclarationEvent::Corrected(payload))
    }
}

fn normalise_correction_set(mut cs: CorrectionSet) -> CorrectionSet {
    cs.metadata_notes = cs
        .metadata_notes
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cs
}

fn validate_belief(field: &'static str, value: f64) -> Result<(), DomainError> {
    if !(0.0..=1.0).contains(&value) || !value.is_finite() {
        return Err(DomainError::FusedBeliefOutOfRange { field, value });
    }
    Ok(())
}

fn validate_command(cmd: &SubmitDeclaration) -> Result<(), DomainError> {
    if cmd.declarant_principal.trim().is_empty() {
        return Err(DomainError::EmptyDeclarantPrincipal);
    }
    if cmd.attestation.signed_by != cmd.declarant_principal {
        return Err(DomainError::AttestationPrincipalMismatch {
            expected: cmd.declarant_principal.clone(),
            actual: cmd.attestation.signed_by.clone(),
        });
    }
    validate_beneficial_owners(&cmd.beneficial_owners)?;
    validate_effective_from(cmd.effective_from, cmd.submitted_at)?;
    // TODO-021: adequacy claims, when present, must satisfy the
    // up-to-date window + non-empty legal basis. Acceptance of the
    // bare absence (None) is currently controlled at the API DTO
    // boundary; the aggregate accepts None for back-compat with
    // historical events that pre-date this migration.
    if let Some(claims) = &cmd.adequacy_claims {
        validate_adequacy_claims(claims, cmd.submitted_at)?;
    }
    // PR-FATF-4 / TODO-005: validate the declarant-asserted BO event
    // timestamp when present. The aggregate accepts None for back-
    // compat; the API DTO layer enforces required-ness on new writes
    // (PR-FATF-4.B).
    if let Some(as_of) = cmd.last_event_observed_at {
        validate_last_event_observed_at(as_of, cmd.submitted_at)?;
    }
    Ok(())
}

/// TODO-005 closure — declarant-asserted "the BO change occurred at"
/// timestamp invariants. The aggregate refuses values that are
/// structurally implausible; the staleness worker (in
/// `infrastructure::staleness`) flags rows that *pass* this
/// validation but are now > 30 days old without an update.
fn validate_last_event_observed_at(
    as_of: OffsetDateTime,
    submitted_at: OffsetDateTime,
) -> Result<(), DomainError> {
    if as_of > submitted_at {
        return Err(DomainError::LastEventObservedAtInFuture {
            as_of,
            submitted_at,
        });
    }
    let five_years_ago = submitted_at - Duration::days(365 * 5);
    if as_of < five_years_ago {
        return Err(DomainError::LastEventObservedAtTooOld {
            as_of,
            submitted_at,
        });
    }
    Ok(())
}

/// TODO-001 + TODO-010 closure — validates the FATF cascade and nominee
/// invariants on the beneficial-owner roster.
fn validate_beneficial_owners(owners: &[BeneficialOwnerClaim]) -> Result<(), DomainError> {
    if owners.is_empty() {
        return Err(DomainError::NoBeneficialOwners);
    }
    let mut seen = HashSet::new();
    let mut sum: u32 = 0;
    let mut has_control_tier = false;
    let mut has_smo_tier = false;
    let registered_ids: HashSet<uuid::Uuid> = owners.iter().map(|o| o.person_id.0).collect();

    for owner in owners {
        if !seen.insert(owner.person_id) {
            return Err(DomainError::DuplicateBeneficialOwner(owner.person_id.0));
        }
        sum = sum.saturating_add(owner.ownership_basis_points.as_basis_points());
        validate_cascade_tier(owner)?;
        validate_nominee_fields(owner, &registered_ids)?;

        match owner.cascade_tier {
            Some(BoCascadeTier::Control) => has_control_tier = true,
            Some(BoCascadeTier::SeniorManagingOfficial) => has_smo_tier = true,
            _ => {}
        }
    }
    if sum != 10_000 {
        return Err(DomainError::OwnershipSumInvariant { sum });
    }
    // FATF cascade is hierarchical: a tier-(c) SMO BO is admissible
    // only when the declarant has *also* identified a tier-(b)
    // Control candidate that was ruled out. The aggregate enforces
    // the structural visibility: when SMO appears, at least one
    // Control candidate must also appear in the same roster. The
    // back-office workflow validates the "ruled-out evidence" string
    // semantically.
    if has_smo_tier && !has_control_tier {
        return Err(DomainError::SmoTierWithoutVisibleControlSearch);
    }
    Ok(())
}

/// TODO-001 closure — per-owner cascade tier consistency.
fn validate_cascade_tier(owner: &BeneficialOwnerClaim) -> Result<(), DomainError> {
    // Legacy sentinel is read-only — never an input.
    if let Some(BoCascadeTier::LegacyPreCascade) = owner.cascade_tier {
        return Err(DomainError::LegacyCascadeTierOnNewDeclaration {
            person_id: owner.person_id.0,
        });
    }

    match owner.cascade_tier {
        Some(BoCascadeTier::Control) => {
            if owner.control_basis.is_none() {
                return Err(DomainError::ControlTierMissingBasis {
                    person_id: owner.person_id.0,
                });
            }
            if owner.cascade_tier_b_ruled_out_evidence.is_some() {
                return Err(DomainError::RuledOutEvidenceOnNonSmoTier {
                    person_id: owner.person_id.0,
                    tier: BoCascadeTier::Control.as_str(),
                });
            }
        }
        Some(BoCascadeTier::SeniorManagingOfficial) => {
            if owner.control_basis.is_some() {
                return Err(DomainError::ControlBasisOnNonControlTier {
                    person_id: owner.person_id.0,
                    tier: BoCascadeTier::SeniorManagingOfficial.as_str(),
                });
            }
            match owner.cascade_tier_b_ruled_out_evidence.as_deref() {
                None => {
                    return Err(DomainError::SmoTierMissingRuledOutEvidence {
                        person_id: owner.person_id.0,
                    });
                }
                Some(s) if s.trim().len() < 16 => {
                    // Minimum 16 chars: an investigator-readable note
                    // (e.g. "no controller found via shareholder agreements after review of M&A 2025").
                    return Err(DomainError::SmoTierMissingRuledOutEvidence {
                        person_id: owner.person_id.0,
                    });
                }
                Some(_) => {}
            }
        }
        Some(BoCascadeTier::OwnershipDirect) | Some(BoCascadeTier::OwnershipIndirect) => {
            if owner.control_basis.is_some() {
                let tier_str = match owner.cascade_tier {
                    Some(BoCascadeTier::OwnershipDirect) => BoCascadeTier::OwnershipDirect.as_str(),
                    Some(BoCascadeTier::OwnershipIndirect) => {
                        BoCascadeTier::OwnershipIndirect.as_str()
                    }
                    _ => "ownership",
                };
                return Err(DomainError::ControlBasisOnNonControlTier {
                    person_id: owner.person_id.0,
                    tier: tier_str,
                });
            }
            if owner.cascade_tier_b_ruled_out_evidence.is_some() {
                let tier_str = match owner.cascade_tier {
                    Some(BoCascadeTier::OwnershipDirect) => BoCascadeTier::OwnershipDirect.as_str(),
                    Some(BoCascadeTier::OwnershipIndirect) => {
                        BoCascadeTier::OwnershipIndirect.as_str()
                    }
                    _ => "ownership",
                };
                return Err(DomainError::RuledOutEvidenceOnNonSmoTier {
                    person_id: owner.person_id.0,
                    tier: tier_str,
                });
            }
        }
        // Cascade tier is None on legacy-replay paths only; the
        // current API DTO refuses None. Accept None here so historical
        // events continue to deserialise without invariant breakage.
        None => {}
        Some(BoCascadeTier::LegacyPreCascade) => unreachable!(),
    }
    Ok(())
}

/// TODO-010 closure — per-owner nominee consistency. When `is_nominee`
/// is `true`, the nominator must be set, must differ from the nominee,
/// and must itself appear on the same declaration as a separately-
/// registered BO (so the nominator is recorded under the cascade).
fn validate_nominee_fields(
    owner: &BeneficialOwnerClaim,
    registered_ids: &HashSet<uuid::Uuid>,
) -> Result<(), DomainError> {
    match (owner.is_nominee, owner.nominator_person_id) {
        (Some(true), None) => Err(DomainError::NomineeMissingNominator {
            person_id: owner.person_id.0,
        }),
        (Some(false) | None, Some(_)) => Err(DomainError::NominatorWithoutNomineeFlag {
            person_id: owner.person_id.0,
        }),
        (Some(true), Some(nominator)) if nominator == owner.person_id => {
            Err(DomainError::SelfNominationForbidden {
                person_id: owner.person_id.0,
            })
        }
        (Some(true), Some(nominator)) => {
            if !registered_ids.contains(&nominator.0) {
                return Err(DomainError::NominatorNotRegisteredOnDeclaration {
                    nominee_id: owner.person_id.0,
                    nominator_id: nominator.0,
                });
            }
            Ok(())
        }
        (Some(false) | None, None) => Ok(()),
    }
}

/// TODO-021 closure — adequacy_claims block invariants.
fn validate_adequacy_claims(
    claims: &super::attestation::AdequacyClaims,
    submitted_at: OffsetDateTime,
) -> Result<(), DomainError> {
    let basis = claims.legal_basis.trim();
    if basis.is_empty() {
        return Err(DomainError::AdequacyLegalBasisEmpty);
    }
    if basis.chars().count() > 1024 {
        return Err(DomainError::AdequacyLegalBasisTooLong);
    }
    if claims.up_to_date_as_of > submitted_at {
        return Err(DomainError::AdequacyAsOfInFuture {
            as_of: claims.up_to_date_as_of,
            submitted_at,
        });
    }
    let thirty_days_ago = submitted_at - Duration::days(30);
    if claims.up_to_date_as_of < thirty_days_ago {
        return Err(DomainError::AdequacyAsOfStale {
            as_of: claims.up_to_date_as_of,
            submitted_at,
        });
    }
    Ok(())
}

fn validate_effective_from(
    effective_from: time::Date,
    submitted_at: OffsetDateTime,
) -> Result<(), DomainError> {
    let submitted_date = submitted_at.date();
    if effective_from > submitted_date {
        return Err(DomainError::EffectiveFromInFuture {
            effective_from,
            submitted_at: submitted_date,
        });
    }
    let five_years_ago = submitted_at - Duration::days(365 * 5);
    if OffsetDateTime::new_utc(effective_from, time::Time::MIDNIGHT) < five_years_ago {
        return Err(DomainError::EffectiveFromTooOld {
            effective_from,
            submitted_at: submitted_date,
        });
    }
    Ok(())
}

/// BLAKE3 hash of the canonical form of the command. Used as the
/// receipt the API returns to the declarant.
fn compute_receipt_hash(cmd: &SubmitDeclaration) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        declaration_id: super::value_object::DeclarationId,
        entity_id: super::value_object::EntityId,
        declarant_principal: &'a str,
        declarant_role: &'static str,
        kind: &'static str,
        effective_from: &'a time::Date,
        beneficial_owners: &'a [BeneficialOwnerClaim],
        signature_hex: &'a str,
        nonce_hex: &'a str,
    }

    let canonical = Canonical {
        declaration_id: cmd.declaration_id,
        entity_id: cmd.entity_id,
        declarant_principal: &cmd.declarant_principal,
        declarant_role: cmd.declarant_role.as_str(),
        kind: cmd.kind.as_str(),
        effective_from: &cmd.effective_from,
        beneficial_owners: &cmd.beneficial_owners,
        signature_hex: &cmd.attestation.signature_hex,
        nonce_hex: &cmd.attestation.nonce_hex,
    };
    // serde_json serialises object keys in insertion order from the
    // derive macro; the field order above is the canonical order.
    let bytes = serde_json::to_vec(&canonical).expect("canonical fields are all serialisable");
    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use time::macros::date;
    use uuid::Uuid;

    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationKind, EntityId, InterestKind,
        OwnershipBasisPoints, PersonId,
    };

    use super::*;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[1u8; 32])
    }

    fn attestation_for(principal: &str) -> CryptographicAttestation {
        let key = signing_key();
        let payload = b"any payload - the attestation byte-payload check is at the API";
        let signature = key.sign(payload);
        CryptographicAttestation {
            signed_by: principal.to_string(),
            signature_algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: hex::encode(signature.to_bytes()),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            nonce_hex: hex::encode([7u8; 16]),
        }
    }

    fn submit_command(
        declaration_id: DeclarationId,
        owners: Vec<BeneficialOwnerClaim>,
    ) -> SubmitDeclaration {
        SubmitDeclaration {
            declaration_id,
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: "spiffe://recor.cm/test-declarant".to_string(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: owners,
            attestation: attestation_for("spiffe://recor.cm/test-declarant"),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            adequacy_claims: None,
            last_event_observed_at: None,
        }
    }

    fn owner(percent_basis_points: u32) -> BeneficialOwnerClaim {
        BeneficialOwnerClaim {
            person_id: PersonId(Uuid::now_v7()),
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(
                percent_basis_points,
            )
            .unwrap(),
            interest_kind: InterestKind::Equity,
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        }
    }

    #[test]
    fn fresh_aggregate_at_version_zero() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        assert_eq!(agg.version, 0);
    }

    #[test]
    fn submit_increments_version() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd).expect("valid submit");
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.version, 1);
        assert_eq!(agg.state, DeclarationState::Submitted);
    }

    #[test]
    fn submit_twice_is_rejected() {
        let mut agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd.clone()).unwrap();
        agg.apply(&event);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::AlreadySubmitted(_)));
    }

    #[test]
    fn no_beneficial_owners_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![]);
        assert_eq!(agg.handle_submit(cmd).unwrap_err(), DomainError::NoBeneficialOwners);
    }

    #[test]
    fn ownership_sum_must_equal_10000() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(5_000), owner(4_000)]);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert_eq!(err, DomainError::OwnershipSumInvariant { sum: 9_000 });
    }

    #[test]
    fn duplicate_owner_rejects() {
        let person = PersonId(Uuid::now_v7());
        let dup = |percent_basis_points: u32| BeneficialOwnerClaim {
            person_id: person,
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(
                percent_basis_points,
            )
            .unwrap(),
            interest_kind: InterestKind::Equity,
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![dup(5_000), dup(5_000)]);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::DuplicateBeneficialOwner(_)));
    }

    #[test]
    fn future_effective_from_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.effective_from = date!(2099 - 12 - 31);
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::EffectiveFromInFuture { .. }));
    }

    #[test]
    fn attestation_principal_must_match() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.attestation = attestation_for("spiffe://recor.cm/different-principal");
        let err = agg.handle_submit(cmd).unwrap_err();
        assert!(matches!(err, DomainError::AttestationPrincipalMismatch { .. }));
    }

    #[test]
    fn receipt_hash_is_stable_for_same_inputs() {
        let id = DeclarationId::new();
        // Hold submitted_at + correlation_id + nonce constant by reusing the
        // command across calls.
        let cmd = submit_command(id, vec![owner(10_000)]);
        let h1 = compute_receipt_hash(&cmd);
        let h2 = compute_receipt_hash(&cmd);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // BLAKE3 default = 32 bytes = 64 hex chars
    }

    fn verify_command(
        agg: &DeclarationAggregate,
        lane: crate::domain::value_object::VerificationLane,
    ) -> RecordVerificationOutcome {
        RecordVerificationOutcome {
            declaration_id: agg.id,
            verification_case_id: Uuid::now_v7(),
            lane,
            fused_authenticity_belief: 0.92,
            fused_authenticity_plausibility: 0.97,
            fused_risk_belief: 0.05,
            completed_at: OffsetDateTime::now_utc(),
        }
    }

    fn submitted_aggregate() -> DeclarationAggregate {
        let mut agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        let event = agg.handle_submit(cmd).unwrap();
        agg.apply(&event);
        agg
    }

    #[test]
    fn record_verification_before_submit_rejects() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::VerificationOutcomeBeforeSubmit(_)));
    }

    #[test]
    fn green_lane_transitions_to_accepted() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let event = agg
            .handle_record_verification(cmd)
            .unwrap()
            .expect("first verification emits an event");
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::Accepted);
        assert!(agg.verification_case_id.is_some());
    }

    #[test]
    fn yellow_lane_transitions_to_in_verification() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Yellow);
        let event = agg.handle_record_verification(cmd).unwrap().unwrap();
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::InVerification);
    }

    #[test]
    fn red_lane_transitions_to_rejected() {
        let agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Red);
        let event = agg.handle_record_verification(cmd).unwrap().unwrap();
        let mut agg = agg;
        agg.apply(&event);
        assert_eq!(agg.state, DeclarationState::Rejected);
    }

    #[test]
    fn replay_same_case_is_noop() {
        let mut agg = submitted_aggregate();
        let cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let case_id = cmd.verification_case_id;
        let event = agg.handle_record_verification(cmd.clone()).unwrap().unwrap();
        agg.apply(&event);
        // Same case_id replayed
        let mut replay = cmd;
        replay.verification_case_id = case_id;
        let result = agg.handle_record_verification(replay).unwrap();
        assert!(result.is_none(), "replay must not produce a second event");
    }

    #[test]
    fn different_case_after_verified_rejects() {
        let mut agg = submitted_aggregate();
        let cmd1 = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let event = agg.handle_record_verification(cmd1).unwrap().unwrap();
        agg.apply(&event);
        let cmd2 = verify_command(&agg, crate::domain::value_object::VerificationLane::Red);
        let err = agg.handle_record_verification(cmd2).unwrap_err();
        assert!(matches!(err, DomainError::VerificationCaseMismatch { .. }));
    }

    #[test]
    fn out_of_range_belief_rejects() {
        let agg = submitted_aggregate();
        let mut cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        cmd.fused_authenticity_belief = 1.5;
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::FusedBeliefOutOfRange { .. }));
    }

    #[test]
    fn nan_belief_rejects() {
        let agg = submitted_aggregate();
        let mut cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        cmd.fused_risk_belief = f64::NAN;
        let err = agg.handle_record_verification(cmd).unwrap_err();
        assert!(matches!(err, DomainError::FusedBeliefOutOfRange { .. }));
    }

    // ─── Amend ────────────────────────────────────────────────────────────

    fn amend_cmd(
        id: DeclarationId,
        principal: &str,
        amendments: AmendmentSet,
    ) -> AmendDeclaration {
        AmendDeclaration {
            declaration_id: id,
            declarant_principal: principal.to_string(),
            amendments,
            attestation: attestation_for(principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn default_amendments() -> AmendmentSet {
        AmendmentSet {
            beneficial_owners: vec![owner(6_000), owner(4_000)],
            effective_from: date!(2026 - 02 - 01),
            declarant_role: DeclarantRole::AuthorisedAgent,
            adequacy_claims: None,
        }
    }

    #[test]
    fn amend_from_submitted_emits_amended_event() {
        let agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().expect("aggregate has principal");
        let amendments = default_amendments();
        let cmd = amend_cmd(agg.id, &principal, amendments.clone());
        let event = agg.handle_amend(cmd).expect("amend allowed from Submitted");
        let DeclarationEvent::Amended(payload) = event else {
            panic!("expected Amended event, got {event:?}");
        };
        assert_eq!(payload.declaration_id, agg.id);
        assert_eq!(payload.after, amendments);
        // before snapshot is the aggregate's amendable-state snapshot
        // populated when the Submitted event was applied.
        let before_owners = &payload.before.beneficial_owners;
        assert_eq!(before_owners.len(), 1);
        assert_eq!(before_owners[0].ownership_basis_points.as_basis_points(), 10_000);
    }

    #[test]
    fn amend_from_in_verification_emits_amended_event() {
        let mut agg = submitted_aggregate();
        let v_cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Yellow);
        let v_event = agg.handle_record_verification(v_cmd).unwrap().unwrap();
        agg.apply(&v_event);
        assert_eq!(agg.state, DeclarationState::InVerification);
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = amend_cmd(agg.id, &principal, default_amendments());
        assert!(matches!(agg.handle_amend(cmd), Ok(DeclarationEvent::Amended(_))));
    }

    #[test]
    fn amend_from_accepted_refused_with_supersede_guidance() {
        let mut agg = submitted_aggregate();
        let v_cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let v_event = agg.handle_record_verification(v_cmd).unwrap().unwrap();
        agg.apply(&v_event);
        assert_eq!(agg.state, DeclarationState::Accepted);
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = amend_cmd(agg.id, &principal, default_amendments());
        let err = agg.handle_amend(cmd).unwrap_err();
        let msg = err.to_string();
        // Message MUST mention Supersede so the API surfaces operator guidance.
        assert!(matches!(err, DomainError::AmendFromInvalidState { .. }));
        assert!(
            msg.contains("Supersede"),
            "AmendFromInvalidState message must mention Supersede; got: {msg}"
        );
    }

    #[test]
    fn amend_from_rejected_refused() {
        let mut agg = submitted_aggregate();
        let v_cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Red);
        let v_event = agg.handle_record_verification(v_cmd).unwrap().unwrap();
        agg.apply(&v_event);
        assert_eq!(agg.state, DeclarationState::Rejected);
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = amend_cmd(agg.id, &principal, default_amendments());
        assert!(matches!(
            agg.handle_amend(cmd).unwrap_err(),
            DomainError::AmendFromInvalidState { .. }
        ));
    }

    #[test]
    fn amend_from_superseded_refused() {
        let mut agg = submitted_aggregate();
        let v_cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Green);
        let v_event = agg.handle_record_verification(v_cmd).unwrap().unwrap();
        agg.apply(&v_event);
        let new_id = DeclarationId::new();
        let sup_event = agg.handle_supersede(new_id, Uuid::now_v7()).unwrap();
        agg.apply(&sup_event);
        assert_eq!(agg.state, DeclarationState::Superseded);
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = amend_cmd(agg.id, &principal, default_amendments());
        assert!(matches!(
            agg.handle_amend(cmd).unwrap_err(),
            DomainError::AmendFromInvalidState { .. }
        ));
    }

    #[test]
    fn amend_preserves_owner_sum_invariant() {
        let agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        // Sum != 10_000 must be refused with the same error as Submit.
        let bad_amendments = AmendmentSet {
            beneficial_owners: vec![owner(5_000), owner(4_000)],
            effective_from: date!(2026 - 02 - 01),
            declarant_role: DeclarantRole::SelfDeclaration,
            adequacy_claims: None,
        };
        let cmd = amend_cmd(agg.id, &principal, bad_amendments);
        assert_eq!(
            agg.handle_amend(cmd).unwrap_err(),
            DomainError::OwnershipSumInvariant { sum: 9_000 }
        );
    }

    #[test]
    fn amend_by_non_owner_refused() {
        let agg = submitted_aggregate();
        let cmd = amend_cmd(agg.id, "spiffe://recor.cm/some-other-principal", default_amendments());
        assert!(matches!(
            agg.handle_amend(cmd).unwrap_err(),
            DomainError::AmendNotOwner { .. }
        ));
    }

    #[test]
    fn two_amendments_in_sequence_both_apply() {
        let mut agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        let first = default_amendments();
        let event1 = agg.handle_amend(amend_cmd(agg.id, &principal, first.clone())).unwrap();
        agg.apply(&event1);
        // The aggregate's amendment_state should now reflect first.
        assert_eq!(agg.amendment_state.as_ref().unwrap(), &first);
        // Second amendment uses a different roster.
        let second = AmendmentSet {
            beneficial_owners: vec![owner(7_000), owner(3_000)],
            effective_from: date!(2026 - 03 - 01),
            declarant_role: DeclarantRole::OperatorAssisted,
            adequacy_claims: None,
        };
        let event2 = agg.handle_amend(amend_cmd(agg.id, &principal, second.clone())).unwrap();
        // before snapshot on the second event must equal the first
        // amendment's `after` (proving the aggregate observed it).
        let DeclarationEvent::Amended(payload2) = &event2 else { panic!(); };
        assert_eq!(payload2.before, first);
        assert_eq!(payload2.after, second);
        agg.apply(&event2);
        assert_eq!(agg.amendment_state.as_ref().unwrap(), &second);
        // Version monotonic increment: Submitted + Amended×2 = version 3.
        assert_eq!(agg.version, 3);
    }

    #[test]
    fn replay_amend_event_reproduces_before_and_after() {
        // The acceptance criterion: replaying the event log reproduces
        // both before and after snapshots.
        let mut agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        let event = agg
            .handle_amend(amend_cmd(agg.id, &principal, default_amendments()))
            .unwrap();
        agg.apply(&event);

        let DeclarationEvent::Amended(payload) = &event else { panic!(); };
        // The Amended payload itself carries both snapshots.
        assert_eq!(payload.after.beneficial_owners.len(), 2);
        assert_eq!(payload.before.beneficial_owners.len(), 1);

        // Now rehydrate a fresh aggregate by replaying a synthesised
        // event stream (Submitted + the Amended event). The replayed
        // aggregate's `amendment_state` must match what we observed
        // after the original `apply` call.
        let snapshot_after_apply = agg.amendment_state.clone().unwrap();
        let mut replayed = DeclarationAggregate::fresh(agg.id);
        replayed.apply(&DeclarationEvent::Submitted(DeclarationSubmittedV1 {
            declaration_id: agg.id,
            entity_id: agg.entity_id.unwrap(),
            declarant_principal: principal.clone(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![owner(10_000)],
            attestation: attestation_for(&principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            receipt_hash_hex: "0".repeat(64),
                    adequacy_claims: None,
                    last_event_observed_at: None,
}));
        replayed.apply(&event);
        assert_eq!(replayed.amendment_state.unwrap(), snapshot_after_apply);
    }

    // ─── Correct ──────────────────────────────────────────────────────────

    fn correct_cmd(
        id: DeclarationId,
        principal: &str,
        corrections: CorrectionSet,
    ) -> CorrectDeclaration {
        CorrectDeclaration {
            declaration_id: id,
            declarant_principal: principal.to_string(),
            corrections,
            attestation: attestation_for(principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn correct_from_submitted_emits_corrected_event() {
        let agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        let corrections = CorrectionSet {
            metadata_notes: Some("Operator typo in supporting docs ref".into()),
        };
        let event = agg.handle_correct(correct_cmd(agg.id, &principal, corrections.clone())).unwrap();
        let DeclarationEvent::Corrected(payload) = event else {
            panic!("expected Corrected event");
        };
        assert_eq!(payload.declaration_id, agg.id);
        assert_eq!(payload.after, corrections);
        assert!(payload.before.metadata_notes.is_none());
    }

    #[test]
    fn correct_from_in_verification_refused_directs_to_amend() {
        let mut agg = submitted_aggregate();
        let v_cmd = verify_command(&agg, crate::domain::value_object::VerificationLane::Yellow);
        let v_event = agg.handle_record_verification(v_cmd).unwrap().unwrap();
        agg.apply(&v_event);
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = correct_cmd(
            agg.id,
            &principal,
            CorrectionSet { metadata_notes: Some("x".into()) },
        );
        let err = agg.handle_correct(cmd).unwrap_err();
        assert!(matches!(err, DomainError::CorrectFromInvalidState { .. }));
        // The error message must direct the operator to Amend or Supersede.
        assert!(
            err.to_string().contains("Amend") && err.to_string().contains("Supersede"),
            "CorrectFromInvalidState message must mention both Amend and Supersede; got: {err}"
        );
    }

    #[test]
    fn correct_metadata_notes_roundtrip_through_apply() {
        // Acceptance criterion: correct metadata_notes round-trips
        // through GET (here we exercise the aggregate's apply path
        // which the projection mirrors).
        let mut agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        let corrections = CorrectionSet {
            metadata_notes: Some("Note for the operator".into()),
        };
        let event = agg.handle_correct(correct_cmd(agg.id, &principal, corrections.clone())).unwrap();
        agg.apply(&event);
        assert_eq!(agg.correction_state, corrections);
        // Replay path: build a fresh aggregate from the synthesised
        // event stream and confirm the same correction_state.
        let DeclarationEvent::Corrected(payload) = &event else { panic!(); };
        let mut replayed = DeclarationAggregate::fresh(agg.id);
        replayed.apply(&DeclarationEvent::Submitted(DeclarationSubmittedV1 {
            declaration_id: agg.id,
            entity_id: agg.entity_id.unwrap(),
            declarant_principal: principal.clone(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![owner(10_000)],
            attestation: attestation_for(&principal),
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id: Uuid::now_v7(),
            receipt_hash_hex: "0".repeat(64),
                    adequacy_claims: None,
                    last_event_observed_at: None,
}));
        replayed.apply(&event);
        assert_eq!(replayed.correction_state, payload.after);
    }

    #[test]
    fn correct_by_non_owner_refused() {
        let agg = submitted_aggregate();
        let cmd = correct_cmd(
            agg.id,
            "spiffe://recor.cm/different",
            CorrectionSet { metadata_notes: Some("note".into()) },
        );
        assert!(matches!(
            agg.handle_correct(cmd).unwrap_err(),
            DomainError::CorrectNotOwner { .. }
        ));
    }

    #[test]
    fn correct_normalises_empty_string_to_none() {
        let agg = submitted_aggregate();
        let principal = agg.declarant_principal.clone().unwrap();
        let cmd = correct_cmd(
            agg.id,
            &principal,
            CorrectionSet { metadata_notes: Some("   ".into()) },
        );
        let event = agg.handle_correct(cmd).unwrap();
        let DeclarationEvent::Corrected(payload) = event else { panic!(); };
        assert!(payload.after.metadata_notes.is_none());
    }

    // ─── PR-FATF-2.A — TODO-001 / -010 / -021 invariants ──────────────
    //
    // Each test exercises one FATF cascade or nominee invariant in
    // isolation. The helpers `owner(...)` and `submit_command(...)` above
    // default the new fields to None; the tests below override the
    // specific field they're exercising.

    use crate::domain::attestation::AdequacyClaims;
    use crate::domain::value_object::{BoCascadeTier, BoControlBasis};

    fn owner_at_tier(percent_basis_points: u32, tier: BoCascadeTier) -> BeneficialOwnerClaim {
        let mut o = owner(percent_basis_points);
        o.cascade_tier = Some(tier);
        match tier {
            BoCascadeTier::Control => {
                o.control_basis = Some(BoControlBasis::VotingRights);
            }
            BoCascadeTier::SeniorManagingOfficial => {
                o.cascade_tier_b_ruled_out_evidence =
                    Some("tier-(b) search exhausted via review of shareholder agreements".into());
            }
            _ => {}
        }
        o
    }

    #[test]
    fn cascade_control_tier_requires_control_basis() {
        let mut o = owner(10_000);
        o.cascade_tier = Some(BoCascadeTier::Control);
        o.control_basis = None;
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::ControlTierMissingBasis { .. }
        ));
    }

    #[test]
    fn cascade_control_basis_refused_on_ownership_tier() {
        let mut o = owner(10_000);
        o.cascade_tier = Some(BoCascadeTier::OwnershipDirect);
        o.control_basis = Some(BoControlBasis::VotingRights);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::ControlBasisOnNonControlTier { .. }
        ));
    }

    #[test]
    fn cascade_smo_tier_requires_ruled_out_evidence() {
        let mut o = owner(10_000);
        o.cascade_tier = Some(BoCascadeTier::SeniorManagingOfficial);
        o.cascade_tier_b_ruled_out_evidence = None;
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::SmoTierMissingRuledOutEvidence { .. }
        ));
    }

    #[test]
    fn cascade_smo_tier_refuses_short_ruled_out_evidence() {
        let mut o = owner(10_000);
        o.cascade_tier = Some(BoCascadeTier::SeniorManagingOfficial);
        // 10 chars — below the 16-char minimum.
        o.cascade_tier_b_ruled_out_evidence = Some("too short.".into());
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::SmoTierMissingRuledOutEvidence { .. }
        ));
    }

    #[test]
    fn cascade_smo_requires_visible_control_search() {
        // SMO BO on its own is refused — the declaration must also
        // carry a Control BO that was ruled out.
        let smo = owner_at_tier(10_000, BoCascadeTier::SeniorManagingOfficial);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![smo]);
        assert_eq!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::SmoTierWithoutVisibleControlSearch
        );
    }

    #[test]
    fn cascade_smo_with_control_succeeds() {
        // A declaration that lists BOTH a Control candidate and an SMO
        // fallback is admissible (the cascade search is visibly
        // documented).
        let control = owner_at_tier(5_000, BoCascadeTier::Control);
        let smo = owner_at_tier(5_000, BoCascadeTier::SeniorManagingOfficial);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![control, smo]);
        let event = agg.handle_submit(cmd).expect("control+SMO admissible");
        let DeclarationEvent::Submitted(_) = event else { panic!() };
    }

    #[test]
    fn cascade_legacy_sentinel_refused_as_input() {
        let mut o = owner(10_000);
        o.cascade_tier = Some(BoCascadeTier::LegacyPreCascade);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::LegacyCascadeTierOnNewDeclaration { .. }
        ));
    }

    #[test]
    fn nominee_missing_nominator_refused() {
        let mut o = owner(10_000);
        o.is_nominee = Some(true);
        o.nominator_person_id = None;
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::NomineeMissingNominator { .. }
        ));
    }

    #[test]
    fn nominator_without_nominee_flag_refused() {
        let mut o = owner(10_000);
        o.is_nominee = Some(false);
        o.nominator_person_id = Some(PersonId(Uuid::now_v7()));
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::NominatorWithoutNomineeFlag { .. }
        ));
    }

    #[test]
    fn self_nomination_refused() {
        let mut o = owner(10_000);
        o.is_nominee = Some(true);
        o.nominator_person_id = Some(o.person_id);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![o]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::SelfNominationForbidden { .. }
        ));
    }

    #[test]
    fn nominator_must_be_registered_on_declaration() {
        let mut nominee = owner(10_000);
        nominee.is_nominee = Some(true);
        let unknown_nominator = PersonId(Uuid::now_v7());
        nominee.nominator_person_id = Some(unknown_nominator);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![nominee]);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::NominatorNotRegisteredOnDeclaration { .. }
        ));
    }

    #[test]
    fn nominee_with_nominator_registered_succeeds() {
        let nominator_id = PersonId(Uuid::now_v7());
        let mut nominator = owner(0); // nominator can hold zero direct interest
        nominator.person_id = nominator_id;
        // Adjust the nominator's BP up so the sum is 10_000:
        nominator.ownership_basis_points =
            OwnershipBasisPoints::try_from_basis_points(5_000).unwrap();
        let mut nominee = owner(0);
        nominee.ownership_basis_points =
            OwnershipBasisPoints::try_from_basis_points(5_000).unwrap();
        nominee.is_nominee = Some(true);
        nominee.nominator_person_id = Some(nominator_id);
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![nominator, nominee]);
        let event = agg.handle_submit(cmd).expect("nominee + nominator admissible");
        let DeclarationEvent::Submitted(_) = event else { panic!() };
    }

    #[test]
    fn adequacy_claims_empty_legal_basis_refused() {
        let claims = AdequacyClaims {
            adequate: true,
            accurate: true,
            up_to_date_as_of: OffsetDateTime::now_utc(),
            legal_basis: "   ".into(),
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.adequacy_claims = Some(claims);
        assert_eq!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::AdequacyLegalBasisEmpty
        );
    }

    #[test]
    fn adequacy_claims_future_as_of_refused() {
        let claims = AdequacyClaims {
            adequate: true,
            accurate: true,
            up_to_date_as_of: OffsetDateTime::now_utc() + Duration::days(1),
            legal_basis: "CEMAC AML/CFT règlement art. 12".into(),
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.adequacy_claims = Some(claims);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::AdequacyAsOfInFuture { .. }
        ));
    }

    #[test]
    fn adequacy_claims_stale_as_of_refused() {
        let claims = AdequacyClaims {
            adequate: true,
            accurate: true,
            up_to_date_as_of: OffsetDateTime::now_utc() - Duration::days(31),
            legal_basis: "CEMAC AML/CFT règlement art. 12".into(),
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.adequacy_claims = Some(claims);
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::AdequacyAsOfStale { .. }
        ));
    }

    #[test]
    fn adequacy_claims_within_window_succeeds() {
        let claims = AdequacyClaims {
            adequate: true,
            accurate: true,
            up_to_date_as_of: OffsetDateTime::now_utc() - Duration::days(7),
            legal_basis: "CEMAC AML/CFT règlement art. 12".into(),
        };
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.adequacy_claims = Some(claims);
        let event = agg.handle_submit(cmd).expect("within-window adequacy admissible");
        let DeclarationEvent::Submitted(p) = event else { panic!() };
        assert!(p.adequacy_claims.is_some());
    }

    #[test]
    fn legacy_owner_without_cascade_tier_accepted_for_backcompat() {
        // Historical declarations replay with `cascade_tier = None`.
        // The aggregate does NOT refuse them — the API DTO layer is
        // where required-ness lives. This test guards forward-compat
        // replay: a 50-event projection rebuild must not blow up
        // because the events pre-date the cascade migration.
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let cmd = submit_command(agg.id, vec![owner(10_000)]);
        // `owner()` sets cascade_tier = None.
        let event = agg.handle_submit(cmd).expect("legacy shape admissible");
        let DeclarationEvent::Submitted(_) = event else { panic!() };
    }

    // ─── PR-FATF-4 / TODO-005 — last_event_observed_at invariants ─────

    #[test]
    fn last_event_observed_at_future_refused() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.last_event_observed_at = Some(cmd.submitted_at + Duration::days(1));
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::LastEventObservedAtInFuture { .. }
        ));
    }

    #[test]
    fn last_event_observed_at_more_than_5_years_old_refused() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.last_event_observed_at = Some(cmd.submitted_at - Duration::days(365 * 5 + 30));
        assert!(matches!(
            agg.handle_submit(cmd).unwrap_err(),
            DomainError::LastEventObservedAtTooOld { .. }
        ));
    }

    #[test]
    fn last_event_observed_at_recent_succeeds() {
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.last_event_observed_at = Some(cmd.submitted_at - Duration::days(7));
        let event = agg.handle_submit(cmd).expect("recent change date admissible");
        let DeclarationEvent::Submitted(p) = event else { panic!() };
        assert!(p.last_event_observed_at.is_some());
    }

    #[test]
    fn last_event_observed_at_absent_succeeds_for_back_compat() {
        // The domain accepts None (legacy / pre-FATF-migration shape).
        // The API DTO layer enforces required-ness on new writes; the
        // aggregate's job is to not break replay of historical events.
        let agg = DeclarationAggregate::fresh(DeclarationId::new());
        let mut cmd = submit_command(agg.id, vec![owner(10_000)]);
        cmd.last_event_observed_at = None;
        let event = agg.handle_submit(cmd).expect("None is admissible at the aggregate");
        let DeclarationEvent::Submitted(p) = event else { panic!() };
        assert!(p.last_event_observed_at.is_none());
    }
}
