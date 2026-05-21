# Data dictionary — discrepancies tables

**Service:** `services/declaration`
**Migration:** `0011_discrepancies.sql`
**Last updated:** 2026-05-20

---

## Table `discrepancies` (current-state projection)

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `discrepancy_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) — FATF R.24 c.24.6(c) discrepancy intake is statutory | Forever — audit record |
| `declaration_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `submitter_obliged_entity_id` | `TEXT NOT NULL` | — | Internal | Art. 6(1)(c) — identity of the submitting institution (not a natural person) | Forever |
| `submitter_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — the natural person at the obliged entity who filed the report | Forever — OHADA AML/CFT carve-out |
| `field_path` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(c) — JSON Pointer (RFC 6901) into the canonical declaration body; no PII of its own but names a PII location | Forever |
| `observed_value` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) — contains the obliged entity's own CDD finding, which references a BO's identity | Forever |
| `expected_value` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) | Forever |
| `evidence_attachment_hash` | `TEXT NULL` | — | Confidential | Art. 6(1)(c) — BLAKE3 digest of evidence bytes held by the obliged entity; no PII embedded in the hash itself | Forever |
| `state` | `TEXT NOT NULL` | CHECK IN ('submitted','triaged','declarant_corrected','discrepancy_invalid','sanction_imposed','escalated') | Public | Art. 6(1)(c) | Forever |
| `submitted_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `resolved_at` | `TIMESTAMPTZ NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `resolution_kind` | `TEXT NULL` | CHECK IN ('declarant_corrected','discrepancy_invalid','sanction_imposed','escalated') | Public | Art. 6(1)(c) | Forever |
| `resolution_notes` | `TEXT NULL` | — | Confidential | Art. 6(1)(c) — operator-authored; may reference PII fragments | Forever |
| `aggregate_version` | `BIGINT NOT NULL` | DEFAULT 0 | Confidential | Art. 6(1)(c) | Forever |

---

## Table `discrepancy_events` (append-only log)

Immutability: BEFORE UPDATE/DELETE/TRUNCATE triggers reuse
`declaration_events_refuse_mutation()` function from migration `0007`.
REVOKE UPDATE, DELETE, TRUNCATE from PUBLIC.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `event_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) | **Forever** — immutable log |
| `discrepancy_id` | `UUID NOT NULL` | FK → `discrepancies.discrepancy_id` | Public | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) — carries the discrepancy detail including `observed_value` and `expected_value` | **Forever** — immutable; OHADA carve-out |
| `actor_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — identity of the actor making the transition | Forever |
| `occurred_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `sequence_no` | `BIGINT NOT NULL` | UNIQUE (discrepancy_id, sequence_no) | Confidential | Art. 6(1)(c) | Forever |

---

## References

- `services/declaration/migrations/0011_discrepancies.sql`
- `docs/compliance/data-classification.md` (COMP-3)
- `docs/adr/0012-sanctions-proportionality-ladder.md` — the sanction path triggered by discrepancy findings
- FATF R.24 c.24.6(c) — obliged-entity discrepancy reporting requirement
