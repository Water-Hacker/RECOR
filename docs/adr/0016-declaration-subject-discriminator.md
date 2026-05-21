# ADR-0016 — Declaration subject discriminator + wire-shape evolution

* Status: Accepted
* Date: 2026-05-20
* Decider: lead-orchestrator (Water Hacker)
* Consulted: architect-reviewer (Opus 4.7), security-reviewer (Opus 4.7)

## Context

FATF R.24 covers beneficial-ownership disclosure for **legal entities**
(companies, NGOs, OHADA GIEs). FATF R.25 covers the same for **legal
arrangements** (express trusts, fiducies, waqf, "similar"). A national
BO registry must accept BOTH — they share the BO-disclosure semantics
but differ in subject kind and (per ADR-0015) live in different parts
of the data model.

The Declaration service shipped with a single `entity_id` field on
every declaration. Wave 1 added migration 0015 to the `declarations`
projection (`subject_kind TEXT DEFAULT 'entity'`, nullable
`arrangement_id`), but left the domain + DTO layers entity-only. Wave 1
A1 explicitly deferred the BeneficialOwnerSubject enum plumbing because
of **byte-parity concerns** (Doctrine 15 — cryptographic provenance):
every portal client in the field today signs canonical JSON bytes that
include `entity_id` in a fixed position. Changing those bytes
invalidates every signature.

This ADR records how the Declaration service closes the
`TODO-002-declaration-link` loop end-to-end while preserving byte-parity
with legacy signatures.

## Decision

### 1. Two canonical shapes, byte-disjoint by construction

The declaration's `canonical_payload_bytes` function emits one of TWO
canonical structs, selected by the resolved subject discriminator:

```text
CanonicalEntity {
    entity_id, declarant_principal, declarant_role, kind,
    effective_from, beneficial_owners, [adequacy_claims], nonce_hex
}
```

```text
CanonicalArrangement {
    arrangement_id, declarant_principal, declarant_role, kind,
    effective_from, beneficial_owners, [adequacy_claims], nonce_hex
}
```

Properties:

* The `CanonicalEntity` shape is byte-identical to what every legacy
  portal client signs today. A byte-parity assertion test
  (`subject_canonical_bytes_tests::legacy_entity_canonical_bytes_are_unchanged`)
  anchors this against a fixed `LEGACY_CANONICAL_BYTES` string
  constant. Any code change that perturbs those bytes fails the test.
* The `CanonicalArrangement` shape OMITS `entity_id` entirely and
  emits `arrangement_id` in its first-field position. The two shapes
  are intentionally non-overlapping: a signature produced over entity
  bytes cannot validate against an arrangement payload, ruling out
  cross-shape attestation reuse by construction (defence-in-depth
  beyond the per-request nonce check).
* `adequacy_claims` uses `skip_serializing_if = "Option::is_none"`
  in both shapes, preserving the PR-FATF-2.B byte-parity guarantee.

### 2. Wire-shape evolution timeline

| Phase | Wire shape (POST /v1/declarations) | Canonical bytes |
|-------|-------------------------------------|-----------------|
| **Legacy / today** | `{ entity_id, declarant_role, kind, … }` (no `subject` field) | `CanonicalEntity` — unchanged |
| **Wave 1 + Wave 2 (now)** | Accepts BOTH legacy + tagged `subject: { kind: "entity", entity_id }` (explicit but redundant) | `CanonicalEntity` — byte-identical to legacy |
| **Wave 2 + portal cutover** | Portal sends `subject: { kind: "arrangement", arrangement_id }` for R.25 declarations | `CanonicalArrangement` |
| **Post-cutover** | Portal continues to send `entity_id` for R.24 and `subject` for R.25 simultaneously; both verified against the byte-disjoint canonical shapes | (per subject kind) |

The legacy `entity_id` field is **NEVER REMOVED** — it remains on
the SubmitDeclarationRequest DTO permanently (a SemVer-style
deprecation is not in scope; the field is the back-compat anchor for
every signature currently in flight).

### 3. Subject-mismatch refusal (D14 fail-closed)

When the request body sends BOTH a tagged `subject: Entity { e }` and
a top-level `entity_id: e'` with `e != e'`, the DTO refuses with
`DtoError::SubjectMismatch`. Refusing rather than silently picking
one defends against:

* a buggy client serialising contradictory values,
* a hostile re-write of one field but not the other after signature.

The canonicalisation function refuses the same condition at the bytes
layer — the attestation cannot be verified against a contradictory
request.

### 4. Domain + projection consistency

* The aggregate carries `Option<BeneficialOwnerSubject>` on both the
  command and event payload (`#[serde(default,
  skip_serializing_if = "Option::is_none")]`). Replay derives
  `Entity { entity_id }` for historical events.
* The SQL projection writes `subject_kind` + `arrangement_id`
  consistently with the migration 0015 CHECK constraint. Read paths
  validate the (subject_kind, arrangement_id) pair via
  `parse_subject()` and surface a `RepositoryError::Backend` on any
  drift.
* `services/declaration/src/application/cascade_tier_resolver.rs`
  switches between R.24 §c.24.6 (Entity) and R.25 §INR.25
  (Arrangement) chains. The aggregate's per-BO `cascade_tier`
  validator runs in both branches.

### 5. Verification engine integration

* `services/verification-engine/src/domain/declaration_snapshot.rs`
  carries `Option<SubjectSnapshot>`. Legacy webhook + Kafka payloads
  deserialise with `None`; Stage 7 derives Entity for back-compat.
* Stage 7 **Rule 6** (TODO-002-declaration-link) fires when the
  current declaration's subject is an arrangement AND the
  cross-entity-ownership graph query returns at least one neighbour
  that lists the same upstream-flagged person_id. BPA mass is held
  at Rule 5's calibration (authenticity 0.15 / 0.45 / 0.40, risk
  0.65 / 0.10 / 0.25) — the cross-subject signal is conceptually
  the same "person appears in two independent contexts" pattern
  with R.24 ↔ R.25 as the disambiguator. An ADR follow-up will
  re-calibrate once a corpus of labelled R.25 cross-subject cases
  is available.

## Doctrine compliance

* **D15 cryptographic provenance** — byte-parity preserved by
  construction (two byte-disjoint canonical structs + an anchor
  test).
* **D14 fail-closed** — subject mismatch refused, SQL CHECK
  constraint refuses inconsistent rows, Stage 7 graph rule degrades
  to vacuous on reader failure rather than fabricating a Pass.
* **D17 zero trust** — subject discriminator does not change the
  declarant principal authentication path; the principal is still
  sourced from auth.
* **D7 no workarounds** — the new wire shape is introduced
  additively; no silent re-write of legacy bytes; no
  ifNotImplementedYet sentinels.

## Alternatives considered

1. **Tag `subject` inline on the legacy canonical struct.**
   Rejected — adding a new field to the legacy struct changes the
   bytes for every existing client. The `skip_serializing_if`
   trick only works if the field is `Option`, and the moment a
   new client opts in, its bytes diverge from a legacy client's —
   exactly the byte-parity break this ADR exists to avoid.

2. **Tagged enum at the top of the canonical struct.** Rejected — a
   tagged enum's first byte is `"kind":"entity"|"arrangement"`,
   which differs from the legacy `"entity_id":` prefix. Same byte-
   parity break.

3. **Refuse legacy clients entirely; require migration.** Rejected
   — every declaration currently in the registry was signed under
   the legacy shape. A migration would require every declarant to
   re-sign every declaration, which is operationally infeasible
   and legally fraught.

4. **Encode subject discriminator in a HTTP header.** Rejected —
   the discriminator is part of the declaration's semantic identity;
   it must be signed alongside the rest of the canonical bytes,
   not carried out-of-band where a forwarder could rewrite it.

## Linked from

* TODOS.md § TODO-002
* services/declaration/migrations/0015_arrangement_subject.sql
* services/declaration/src/domain/beneficial_owner_subject.rs
* services/declaration/src/api/rest.rs::canonical_payload_bytes
* ADR-0010 — FATF BO cascade + adequacy
* ADR-0014 — Stage 7 cross-source decision rules
* ADR-0015 — R.25 arrangements bounded context
