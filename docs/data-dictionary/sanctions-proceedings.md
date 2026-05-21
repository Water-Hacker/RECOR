# Data dictionary — sanctions tables

**Service:** `services/declaration`
**Migration:** `0014_sanctions_proceedings.sql`
**ADR:** `docs/adr/0012-sanctions-proportionality-ladder.md`
**Last updated:** 2026-05-20

---

## Table `sanctions_proceedings` (current-state projection)

Stores the current state of each sanctions proceeding opened against an
entity or declaration for failure to comply with BO obligations. The
ladder progression is documented in ADR-0012; the `sanction_events` log
is the append-only audit trail of each transition.

At least one of `declaration_id` or `entity_id` must be non-NULL
(CHECK constraint `sanctions_target_present`).

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `proceeding_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) — FATF R.24 c.24.13 proportionate sanctions; statutory obligation | Forever — enforcement record |
| `declaration_id` | `UUID NULL` | — | Public | Art. 6(1)(c) | Forever |
| `entity_id` | `UUID NULL` | — | Public | Art. 6(1)(c) | Forever |
| `reason_code` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `state` | `TEXT NOT NULL` | CHECK IN ('submitted','reminder','fined','suspended','referred','public_listed','withdrawn') | Public | Art. 6(1)(c) | Forever |
| `tier` | `INT NULL` | CHECK tier IS NULL OR tier BETWEEN 1 AND 5 | Public | Art. 6(1)(c) — fine tier level | Forever |
| `initiated_by` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — the operator who opened the proceeding | Forever |
| `initiated_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `last_transition_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `last_actor` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — the operator who made the last transition; required for audit trail | Forever |
| `last_justification` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(c) — D14 requires non-empty justification on every ladder transition | Forever |
| `public_listed_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) + Art. 6(1)(e) — public transparency | Forever |
| `public_listing_name` | `TEXT NULL` | — | Public | Art. 6(1)(c) + Art. 6(1)(e) — entity name on the Sovim-balanced public list; entity names are statutory public record | Forever |
| `public_listing_reason` | `TEXT NULL` | — | Public | Art. 6(1)(c) + Art. 6(1)(e) | Forever |
| `withdrawn_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) | Forever |
| `aggregate_version` | `BIGINT NOT NULL` | DEFAULT 0 | Confidential | Art. 6(1)(c) | Forever |

### Notes on `public_listing_name` and post-Sovim balancing

`public_listing_name` contains the entity's legal name (not a natural
person's name). Entity names are classified Public and are not subject
to the post-Sovim natural-person redaction obligation. The platform
refuses to set `public_listing_name` to a value that looks like a
natural-person name (format check at the API boundary; see ADR-0012).

### Notes on `last_justification`

Every ladder transition must carry a non-empty justification string.
This is enforced by the sanctions endpoint handler (D14 fail-closed)
and is stored here to provide the auditor with the documented reason
for each escalation without replaying the full event log.

---

## Table `sanction_events` (append-only audit log)

Immutability: BEFORE UPDATE/DELETE/TRUNCATE triggers reuse
`declaration_events_refuse_mutation()`. REVOKE UPDATE, DELETE, TRUNCATE
from PUBLIC.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `event_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) | **Forever** — immutable |
| `proceeding_id` | `UUID NOT NULL` | FK → `sanctions_proceedings.proceeding_id` | Public | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `payload` | `JSONB NOT NULL` | — | Confidential | Art. 6(1)(c) — carries the transition's input including `justification`; may reference entity IDs but not natural-person PII directly | Forever |
| `actor_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — the operator who executed the transition | Forever |
| `justification` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(c) | Forever |
| `occurred_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `sequence_no` | `BIGINT NOT NULL` | UNIQUE (proceeding_id, sequence_no) | Confidential | Art. 6(1)(c) | Forever |

---

## References

- `services/declaration/migrations/0014_sanctions_proceedings.sql`
- `docs/adr/0012-sanctions-proportionality-ladder.md`
- `docs/compliance/data-classification.md` (COMP-3)
- FATF R.24 c.24.13 — proportionality requirement
