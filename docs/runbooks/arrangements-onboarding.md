# Runbook: onboarding an R.25 arrangement

**Audience:** RÉCOR back-office operators; obliged-entity onboarding desks.
**Scope:** FATF Recommendation 25 / INR.25 — registering a legal
arrangement (express trust, fiducy, waqf, or R.25-similar
arrangement) into RÉCOR.
**Related:** ADR-0015 (R.25 bounded-context decision), migration
`services/entity-service/migrations/0003_arrangements_r25.sql`,
permission matrix § `services/entity-service` — arrangements.

## What this runbook covers

The operator-side workflow to admit an R.25 arrangement into the
registry: settlor identity verification, trustee credential check,
the API surface, and the failure modes the operator must handle.

This runbook is **not** the declarant-portal flow; that surface is
separate and lands in the `TODO-002-portal` follow-up. Use this
runbook when an arrangement is being registered through the
back-office (sovereign-operator-assisted intake, court order,
notary lodgement, or post-incident reconstruction).

## Prerequisites

1. Operator is on the `ADMIN_PRINCIPALS` allowlist for
   `recor-entity-service` (required only to dissolve; registration
   accepts any authenticated principal).
2. The settlor's natural-person identity is registered in
   `services/person-service` (the JSONB `settlor_refs[*].person_id`
   resolves against that service at verification time).
3. For each trustee, ONE of the following identity proofs is in hand:
   - a **person_id** in person-service (natural-person trustee), OR
   - an **entity_id** in entity-service (legal-person trustee), OR
   - a **fiduciary_registration_id** — an external regulator's
     trustee registration handle (TCSP, notary, lawyer-in-trust),
     1..=128 characters; format is jurisdiction-specific.

## R.25 INR §3 mandatory identifier roles

The aggregate refuses the registration if any of these invariants
fail; the operator must collect the data before calling the API.

| Role | Column | Min cardinality | Identity proof |
|---|---|---|---|
| Settlor | `settlor_refs` | ≥ 1 | person_id only |
| Trustee | `trustee_refs` | ≥ 1 | person_id OR entity_id OR fiduciary_registration_id (exactly one per entry) |
| Protector | `protector_refs` | 0+ | person_id only |
| Named beneficiary | `named_beneficiary_refs` | 0+ | person_id only |
| Class beneficiary | `class_beneficiary_specs` | 0+ | free-form class label + criteria object |
| Other natural persons exercising ultimate effective control | `control_exercise_refs` | 0+ | person_id + free-form `control_basis` text (1..=512 chars) |

R.25 INR §3.a + §3.b are aggregate-level — a registration with zero
settlors or zero trustees is refused with `400 bad_request` (kind
`no_settlor` / `no_trustee`). The same applies to update; an update
cannot reduce the roster below those minima.

## Step 1 — Identity verification (settlor)

The settlor is the natural person who created the arrangement.
Before calling the API:

1. Resolve the settlor's `person_id` from person-service. If absent,
   register the person first (`POST /v1/persons` against the person
   service); the response carries the `person_id` to embed.
2. Verify the settlor's identity document (national ID / passport)
   against the person-service record. Annotate the
   `settlor_refs[*].role_metadata` with a short qualifier
   ("originator", "co-settlor", "trust-deed-2024-03-15") so a future
   investigator can correlate against the deed.

## Step 2 — Trustee credential check

For each trustee:

- **Natural-person trustee.** Verify the person is registered, of
  age, and not subject to a sanctions ladder (see
  `docs/runbooks/sanctions-ladder.md`). Embed `person_id` only.
- **Legal-person trustee.** Resolve `entity_id` from entity-service.
  Verify the entity's mandate authorises trustee duties (a SARL
  whose statutes prohibit trustee work is refused at this step).
  Embed `entity_id` only.
- **Registered fiduciary.** Verify the regulator's registration is
  current (consult the regulator's published list; cache only the
  registration handle in `fiduciary_registration_id`). Refuse if
  the registration has expired in the jurisdiction. Embed
  `fiduciary_registration_id` only.

**Failure mode.** If a single trustee entry sets two discriminators
(say `person_id` AND `entity_id`), the API returns
`400 bad_request` with kind `trustee_ref_shape`. The fix is to split
the entry into two trustees; do not merge identities into a single
JSONB row.

## Step 3 — Call `POST /v1/arrangements`

```http
POST /v1/arrangements HTTP/1.1
Host: entity-service.recor.cm
Authorization: Bearer <oidc-token>
Idempotency-Key: <opaque-uuid-or-operator-correlation-id>
Content-Type: application/json

{
  "arrangement_kind": "express_trust",
  "governing_law_jurisdiction": "CM",
  "constitution_date": "2024-06-01",
  "fields": {
    "settlor_refs": [
      { "person_id": "0192...", "role_metadata": "originator" }
    ],
    "trustee_refs": [
      { "person_id": "0193..." },
      { "fiduciary_registration_id": "TCSP/CH/Z/12345" }
    ],
    "protector_refs": [],
    "named_beneficiary_refs": [],
    "class_beneficiary_specs": [
      { "class": "grandchildren-born-after-2030", "criteria": {} }
    ],
    "control_exercise_refs": []
  }
}
```

Replay on the same `Idempotency-Key` + same body returns the original
response. Replay on the same key + different body returns `409
idempotency_conflict` — do **not** retry by stripping the header;
diagnose the original divergence first (D7).

## Step 4 — Updates after registration

Use `POST /v1/arrangements/{id}/update` to replace the entire role
roster (the update endpoint is a full-replacement edit on the R.25
identifier-role columns, not a delta).

Operator workflows that commonly trigger an update:

- A **successor trustee** is appointed (replace `trustee_refs`).
- A **named beneficiary** is born / dies (add or remove the
  `named_beneficiary_refs` entry).
- A **control-exercise finding** surfaces from a discrepancy
  investigation (append a `control_exercise_refs` entry with the
  `control_basis` documented).

The aggregate refuses updates on a dissolved arrangement
(`409 conflict`, kind `update_on_dissolved`). For a corrective
update post-dissolution, raise a manual ticket with the data-team
on-call.

## Step 5 — Dissolution and R.25 INR §3.f retention

Use `POST /v1/arrangements/{id}/dissolve` with the dissolution date
(`dissolution_date`). The service:

1. Refuses if `dissolution_date <= constitution_date`
   (`409 conflict`, kind `dissolution_before_or_equal_constitution`).
2. Refuses if the caller is not on `ADMIN_PRINCIPALS`
   (`403 forbidden`).
3. Refuses if `ADMIN_PRINCIPALS` is empty (`503` — endpoint
   disabled). D14 fail-closed.
4. Computes `retention_until = dissolution_date + 5 years` and
   stores it on the projection.

The retention deadline is a **forward indicator** for the back-
office: arrangements eligible for post-cessation pruning surface in
the Grafana panel `recor_arrangements_retention_due`. The retention
worker itself only prunes `outbox` rows; the event log and
projection are retained indefinitely per COMP-2.

## Common failure modes

| Symptom | Status | Kind | Operator action |
|---|---|---|---|
| `R.25 INR §3.a refuses an arrangement with no settlor` | 400 | `no_settlor` | Add at least one settlor_refs entry; identity-verify it first |
| `R.25 INR §3.b refuses an arrangement with no trustee` | 400 | `no_trustee` | Add at least one trustee_refs entry with exactly one identity discriminator |
| `trustee ref has N discriminators set` | 400 | `trustee_ref_shape` | Split the entry; one trustee = one discriminator |
| `constitution_date ... is in the future` | 400 | `constitution_date_in_future` | The deed cannot post-date NOW(); verify the date against the deed |
| `arrangement ... already registered` | 409 | `conflict` | Duplicate registration; consult `GET /v1/arrangements/{id}` to confirm the existing record matches |
| `arrangement ... is dissolved` | 409 | `conflict` | The arrangement has been dissolved; updates are refused. For a corrective re-registration, see the data-team runbook |
| `optimistic concurrency conflict` | 409 | `optimistic_concurrency_conflict` | Two writers raced; reload the aggregate via GET and retry |
| `dissolve endpoint disabled (admin allowlist empty)` | 503 | `bad_request` | The deployment has no `ADMIN_PRINCIPALS` configured; coordinate with the platform team |

## D17 / D18 reminders

- The caller's principal is sourced from the OIDC subject claim or
  the SPIFFE peer ID — **never** from the request body. The
  `registered_by_principal` field on the event log is populated from
  auth context; do not attempt to pass it through the DTO.
- Identity proofs (national-ID numbers, passport numbers) are
  **never** logged. The operator's intake notes carry them inside
  the case-management system; only the resolved `person_id` /
  `entity_id` lands in `*_refs`.

## Verification after registration

1. `GET /v1/arrangements/{id}` returns the full projection.
2. `version` is `1` immediately after Register; `2` after the first
   Update; etc. A mismatch indicates an out-of-band write — alert
   on-call and freeze further writes against the arrangement.
3. `retention_until` is `null` until dissolution. A non-null value
   on a non-dissolved arrangement indicates corruption — alert
   on-call and consult `arrangement_events` to replay.

## Escalation

- Schema or invariant question → reach out to the domain team.
- Suspected duplicate trustee credentials across multiple
  arrangements → escalate to FIU coordination (`recor:fiu-anif`
  scope holder).
- COMP-2 trigger fired (event log refused mutation) → preserve
  every log line and escalate immediately; the platform's audit
  position depends on the event log being inviolable.
