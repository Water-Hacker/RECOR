# ADR-0015 — R.25 arrangements (trusts, fiducies) bounded context

- **Status:** Accepted (2026-05-20)
- **Deciders:** Domain team, Legal counsel, Lead architect
- **Closes:** TODO-002 (data substrate + ADR; REST + handler stubs
  in this PR, full domain layer in `TODO-002-domain` follow-up)
- **Related:** ADR-0010 (FATF cascade + adequacy), ADR-0012
  (sanctions ladder)

## Context

FATF Recommendation 25 + INR.25 require that countries maintain
adequate, accurate, and up-to-date information on **trustees of any
express trust governed under their law** — and the same obligation
extends to "any similar legal arrangement". The recommendation
specifically names the six identifier roles that must be captured:

1. The **settlor**(s)
2. The **trustee**(s)
3. The **protector**(s) (where applicable)
4. The **beneficiaries** (named)
5. The **class-described beneficiaries** (where individuals are not
   named)
6. **Any other natural persons exercising ultimate effective control**
   over the arrangement

R.25 INR §3.f additionally requires **5-year-after-cessation
retention**: when an arrangement terminates, the records must be
held for at least five years.

The pre-TODO-002 platform had `entities` (legal persons) and
`persons` (natural persons). It had no arrangement concept at all.
On a FATF MER review, this is a **Non-Compliant** rating on R.25 in
its entirety — half the BO obligation.

The architecture decision is: **separate bounded context (new
service) OR discriminated section of the existing entity service**.

## Decision: discriminated section of entity-service

Add an `arrangements` table to entity-service (migration 0003) and
expose it under `/v1/arrangements`. The discriminator is the URL
prefix; the data layer has separate tables (`arrangements` +
`arrangement_events`) so the schema constraints can be R.25-specific
without compromising the entity schema.

The Person service remains the source of natural-person identities
referenced by the arrangement's role columns
(`settlor_refs`, `trustee_refs`, etc., all JSONB arrays of
`person_id` / `entity_id` references).

## Rationale

**Why not a separate `arrangement-service`?** R.25 arrangements
share four properties with legal entities:

1. They have a constitution date + an optional dissolution date.
2. They have admin actions (register, update, dissolve) that follow
   the same lifecycle.
3. They have a per-jurisdiction governing-law dimension.
4. They are referenced from declarations as the *subject* of a BO
   record — the same shape as an entity.

The operational surface (authentication, OIDC, outbox, COMP-2,
admin allowlist, retention worker) is identical to entity-service.
A second service that duplicates all of that adds maintenance
burden for no architectural gain. The differences are at the data
layer; isolating them there is sufficient.

**Why not just discriminate inside the `entities` table itself?**
The FATF R.25 INR §3 schema obligations are categorically different
from R.24 c.24.8. A trust has settlors and trustees; a legal entity
has shareholders and directors. Squeezing both into a single table
with nullable columns + a discriminator would let invalid combinations
(e.g. a "company" with a "settlor") slip through; a separate table
with the appropriate CHECK constraints prevents that at the schema
level.

**Why JSONB arrays for the role columns instead of join tables?**
- Arrangements typically have ≤5 individuals per role; a join table
  optimises for many-to-many at the cost of every read going through
  a join. JSONB stores the references inline; reads are single-row.
- The role-reference structure carries more than just an id: it
  includes the relationship qualifier ("co-settlor", "successor
  trustee", "trustee since 2024-01-01"). JSONB captures the shape
  natively without a per-relationship join-table schema.

**Why is the retention obligation captured by a `retention_until`
column rather than enforced by a trigger?** The retention worker
(`infrastructure/retention.rs`) only touches `outbox`. The
arrangement event log + projection are retained indefinitely per
the same COMP-2 discipline as `entity_events`. The
`retention_until` column is a **forward indicator** to the back-
office that a given arrangement may eventually be eligible for
post-cessation pruning; INR §3.f's five-year minimum is set by the
back-office on dissolution, and the column makes the deadline
visible to operator dashboards.

## R.25-specific schema choices

| Role | JSONB shape | Notes |
|---|---|---|
| `settlor_refs` | `[{"person_id": "<uuid>", "role_metadata": {...}}]` | The natural person who created the trust |
| `trustee_refs` | `[{"person_id": "<uuid>"} \| {"entity_id": "<uuid>"} \| {"fiduciary_registration_id": "<text>"}]` | R.25 admits legal-person trustees + registered fiduciaries |
| `protector_refs` | `[{"person_id": "<uuid>"}]` | Where the deed names a protector |
| `named_beneficiary_refs` | `[{"person_id": "<uuid>"}]` | Beneficiaries named in the deed |
| `class_beneficiary_specs` | `[{"class": "string", "criteria": {...}}]` | Class-defined beneficiaries ("my grandchildren born after 2030") |
| `control_exercise_refs` | `[{"person_id": "<uuid>", "control_basis": "..."}]` | R.25 catch-all — any natural person with ultimate effective control |

The CHECK constraint on `arrangement_kind` enumerates the four
canonical kinds (`express_trust`, `fiducy`, `waqf`, `similar`). New
arrangement kinds (e.g. Liechtenstein Anstalt) add a value to the
CHECK; ALTER TABLE with `DROP CONSTRAINT ... ADD CONSTRAINT ...`
is the standard pattern.

## Consequences

### Positive

- R.25 INR §3 compliance: the platform can answer "do you maintain
  information on every express trust governed under Cameroonian
  law" with a SELECT against `arrangements WHERE governing_law_jurisdiction = 'CM'`.
- The 5-year retention obligation is visible as a per-row deadline
  that operators can dashboard against.
- The discriminated-section choice keeps the operational surface
  (auth, OIDC, outbox, metrics) unified, halving the maintenance
  burden vs a separate service.

### Negative

- The arrangement domain + REST layer is **not yet implemented** in
  this PR. The migration + ADR ship the architectural foundation;
  the REST endpoints (`POST /v1/arrangements`, etc.) + domain
  invariants are the `TODO-002-domain` follow-up. Until that
  follow-up lands, the platform CAN store arrangement rows via
  direct SQL but cannot accept them via the API — the audit
  position is documented but the live surface is not yet open.
- Declarations cannot YET reference an arrangement as a BO
  subject; the declaration's `entity_id` column constraint refers
  to `entities`. The `TODO-002-declaration-link` follow-up adjusts
  the FK + the cascade-tier resolver to admit arrangement-id
  references.

### Operator burden

- The back-office workflow must be extended to admit arrangements
  in the registration flow. The `TODO-002-portal` follow-up adds
  the portal forms.
- Existing entity-service Helm / Kubernetes manifests do not need
  changes — the new table is part of the same service binary.

## R.25 specifically-named obligations and how this ADR maps

| R.25 obligation | Where it lands in this design |
|---|---|
| Identify trustees | `trustee_refs` JSONB |
| Identify settlors | `settlor_refs` JSONB |
| Identify protectors | `protector_refs` JSONB |
| Identify beneficiaries (named) | `named_beneficiary_refs` JSONB |
| Identify class-described beneficiaries | `class_beneficiary_specs` JSONB |
| Identify "other natural persons exercising ultimate effective control" | `control_exercise_refs` JSONB |
| Adequate, accurate, up-to-date | TODO-005 staleness watcher applied to the projection (`TODO-002-staleness` follow-up wires the watcher to the new table) |
| 5-year retention post-cessation | `retention_until` column; back-office workflow sets it on dissolution |
| COMP-2 audit immutability | `arrangement_events` table + triggers |

## Alternatives considered

1. **Separate `services/arrangement-service`.** Rejected: see
   "Why not a separate `arrangement-service`" rationale.
2. **Single `entities` table with discriminator.** Rejected: see
   "Why not just discriminate inside the `entities` table itself".
3. **Reuse the declarations service for arrangements.** Rejected:
   declarations are *reports about* arrangements; arrangements
   themselves are the *subject* of declarations. Conflating the
   two breaks the data model.

## Linked from

- TODOS.md § TODO-002
- services/entity-service/migrations/0003_arrangements_r25.sql
- services/entity-service/CLAUDE.md
- Architecture V4 P14 § Entity Service (forthcoming amendment)
