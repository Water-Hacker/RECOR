# ADR-0010 — FATF R.24 BO cascade, bearer-share + nominee disclosure, adequacy claims

**Status:** accepted
**Date:** 2026-05-20
**Deciders:** Lead architect, security reviewer
**Closes:** TODO-001, TODO-010, TODO-018, TODO-021 from `TODOS.md` (P0)
**Related:** [0001 event-sourcing](0001-event-sourcing-declaration-aggregate.md),
[0005 HMAC rotation](0005-hmac-channel-rotation.md),
[0008 SPIFFE mTLS](0008-spiffe-mtls.md),
[0009 Fabric audit anchoring](0009-fabric-audit-anchoring.md)

## Context

The production-readiness audit (PR #127, `TODOS.md`) identified four
P0 findings that block any claim of FATF R.24 compliance:

- **TODO-001** — the platform's `BeneficialOwnerClaim` treats every BO
  as a percentage-ownership holder. FATF R.24 §c.24.6 requires the
  cascade **ownership → control → senior managing official (SMO)**. A
  registry that cannot distinguish a controlling shareholder from a
  senior managing official fails IO.5 Core Issue 5.1.
- **TODO-010** — FATF R.24 §c.24.12 requires bearer-share + nominee
  disclosure. The schema has neither field.
- **TODO-018** — FATF R.24 §c.24.1(d) fn 15 requires a "sufficient
  link" test for foreign legal persons. The entity schema has none.
- **TODO-021** — FATF R.24 §c.24.8 requires BO data to be *adequate,
  accurate, and up-to-date*. The platform's Ed25519 attestation
  signs the canonical declaration bytes but the declarant doesn't
  explicitly assert these three properties. Without an explicit
  claims block, sanctions-for-non-compliance (TODO-004) cannot
  demonstrate perjury when a claim is later shown false.

## Decision

We extend the declaration domain model with three structural additions:

### 1. `BeneficialOwnerClaim` carries the FATF cascade

New per-owner fields (all `Option<T>` for back-compat with historical
events that pre-date this migration):

- `cascade_tier: Option<BoCascadeTier>` — `OwnershipDirect |
  OwnershipIndirect | Control | SeniorManagingOfficial |
  LegacyPreCascade` (the last is a read-time sentinel; never set on
  new declarations).
- `control_basis: Option<BoControlBasis>` — `VotingRights |
  BoardAppointment | ContractualControl | FamilyAggregation |
  OtherDocumented`. Required iff `cascade_tier == Control`.
- `cascade_tier_b_ruled_out_evidence: Option<String>` — free-text
  evidence (≥ 16 chars) of how the tier-(b) Control search was
  exhausted. Required iff `cascade_tier == SeniorManagingOfficial`.

Aggregate invariants (validated in
`services/declaration/src/domain/aggregate.rs::validate_cascade_tier`
and `validate_beneficial_owners`):

- Control tier requires `control_basis` to be set.
- SMO tier requires `cascade_tier_b_ruled_out_evidence` (length ≥ 16).
- Ownership tiers refuse `control_basis` and `ruled_out_evidence`
  (those fields are tier-specific).
- An SMO BO is only admissible if the same declaration *also* lists at
  least one Control-tier BO (so the tier-(b) search is visibly
  documented alongside the tier-(c) fallback).
- `LegacyPreCascade` is read-only — passing it as input is refused
  (`DomainError::LegacyCascadeTierOnNewDeclaration`).

### 2. `BeneficialOwnerClaim` carries nominee disclosure

New per-owner fields:

- `is_nominee: Option<bool>` — does this BO act on behalf of a
  nominator?
- `nominator_person_id: Option<PersonId>` — the nominator. Required iff
  `is_nominee == Some(true)`.

Aggregate invariants (in `validate_nominee_fields`):

- `is_nominee == Some(true)` requires `nominator_person_id` to be set.
- `nominator_person_id` requires `is_nominee == Some(true)` (no
  ambiguity about whether the field is meaningful).
- Self-nomination is refused (`person_id` cannot equal
  `nominator_person_id`).
- The nominator MUST appear as a separately-registered BO on the same
  declaration. This ensures the nominator is recorded under the FATF
  cascade — they can't appear only as a "nominator-only" reference
  without being themselves disclosed.

### 3. `CryptographicAttestation` is accompanied by an `AdequacyClaims` block

New per-declaration claims (lives on the event payload and the
declarations projection's `adequacy_claims` JSONB column, migration
0009):

```json
{
  "adequate": true,
  "accurate": true,
  "up_to_date_as_of": "2026-04-22T10:00:00Z",
  "legal_basis": "CEMAC AML/CFT règlement art. 12"
}
```

Aggregate invariants (in `validate_adequacy_claims`):

- `legal_basis` non-empty, ≤ 1024 chars.
- `up_to_date_as_of` ≤ submission time (not future-dated).
- `up_to_date_as_of` ≥ submission time minus 30 days (FATF c.24.8
  fn 29 benchmark — updates within one month of any change).

### 4. `entities` carries bearer-share disclosure (TODO-010 §entity side)

Migration 0002 on entity-service:

- `has_outstanding_bearer_shares: BOOLEAN NOT NULL DEFAULT false`
- `bearer_share_status: TEXT NOT NULL DEFAULT 'none'` —
  `none | outstanding | converted | immobilised`
- Cross-field CHECK: `has_outstanding_bearer_shares = true ⇔
  bearer_share_status = 'outstanding'`.

### 5. `entities` carries sufficient-link disclosure (TODO-018)

Migration 0002 on entity-service:

- `sufficient_link_kind: TEXT NULL` — one of `branch |
  significant_business | financial_relationship | real_estate |
  employees | tax_residence | other_documented`.
- `sufficient_link_evidence: TEXT NULL` — 16–2048 chars.
- CHECK constraint: `jurisdiction = 'CM' OR sufficient_link_kind IS
  NOT NULL` (NOT VALID at migration time so existing rows aren't
  punished; future writes enforce).

## Why these choices

**Why `Option<T>` for the new BO fields rather than required?**
Backwards-compatibility with historical event log + replay (D15).
Old `Submitted` / `Amended` / `Corrected` events pre-date this
migration; replay through `serde_json::from_value` would otherwise
fail. `#[serde(default)]` deserialises legacy events with `None` for
every new field; new submissions carry the values. The API DTO layer
(PR-FATF-2.B) refuses absent fields on the *write* path — the
domain accepts None on the *read* path so the projection rebuild is
forward-compatible.

**Why no event-version bump (Submitted V2 etc.)?** Additive Option
fields with `#[serde(default)]` are forward-and-backward compatible
without a version bump. A version bump would force every consumer (V-
engine, audit-verifier, fabric-bridge) to handle two shapes; the
single-version approach keeps the wire shape stable while letting the
fields evolve.

**Why a structural "SMO requires a Control candidate" invariant?**
The FATF cascade is hierarchical — tier (c) is the residual fallback
after (a) and (b) have been searched. Without a structural
representation of the search, a declarant could file a declaration
with only SMO BOs and never show their work. The aggregate enforces
visibility: when SMO appears, at least one Control candidate must also
appear. This isn't a substitute for the back-office review (which
validates semantics), but it forces the declarant to make the cascade
search visible.

**Why a 30-day staleness window on `up_to_date_as_of`?** FATF c.24.8
fn 29 sets the explicit benchmark "updates within one month of any
change". An attestation that asserts "I assert this data was up-to-
date six months ago" is not actually asserting up-to-date; it's
asserting historical accuracy. The 30-day window forces the
declarant to attest only to the recent state.

**Why NOT VALID on the foreign-entity sufficient-link constraint?**
The migration runs against an existing projection that contains rows
predating this design. NOT VALID applies the constraint only to new
writes; historical rows aren't punished. Back-fill is the operator's
job under the BUNEC integration (R-VER-1) when authoritative
provenance for historical rows becomes available.

## Consequences

**Positive:**

- The platform's data model is FATF-cascade-shaped going forward.
- Sanctions workflow (TODO-004) gets a perjury-grade evidence trail
  (declarant signs `adequacy_claims` along with the canonical body).
- Bearer-share + nominee disclosure is no longer attestation-only;
  it's a structural property of the persisted row.

**Negative:**

- API DTO + portal form + canonical-payload regeneration are deferred
  to PR-FATF-2.B (it makes for a clean two-step delivery but the
  end-to-end behaviour change requires both PRs to land).
- gRPC proto contract needs a bump (R-DECL-PROTO-FATF follow-up) to
  carry the cascade fields. Until then gRPC ingestion emits legacy-
  shape BOs (`cascade_tier = None`).
- V-engine's internal wire type `BeneficialOwnerWire` doesn't yet
  carry the cascade — Stage 7 cross-source verification (TODO-013)
  picks this up.

**Neutral:**

- Migration 0009 + 0002 are additive: rollback by dropping the new
  columns is mechanical and loss-free for the new fields.
- Historical event replay continues to work because the new fields
  are `Option<T>` with `#[serde(default)]`.

## Alternatives considered

**Bump event version (Submitted V2).** Rejected — forces every
consumer to handle two shapes for no structural benefit beyond
"semver-clean". Additive Option fields achieve the same forward
compatibility.

**Make new fields required at the domain layer.** Rejected — breaks
replay of historical events. The API DTO is where required-ness
belongs; the domain accepts the legacy shape and refuses it only on
fresh submissions via the DTO validator (PR-FATF-2.B).

**Put `adequacy_claims` inside `CryptographicAttestation`.** Rejected
— would change the attestation type's serde shape, which is signed
material. The current design keeps the attestation type stable;
`adequacy_claims` is a peer field in the canonical payload (after
`beneficial_owners`, before `nonce_hex`), so it's included in the
signed bytes without restructuring the attestation type.

**Compute the SMO-needs-visible-Control invariant at the back-office
review stage rather than at the aggregate.** Rejected — the aggregate
is the closest enforcement point to the canonical payload, and the
invariant is structural rather than semantic. Pushing it to the back-
office means the audit log accepts SMO-only declarations that fail
review — operational mess.
