# RÉCOR — Field-level data dictionary

**Ticket:** TODO-081
**Owner:** @recor/domain-team
**Last updated:** 2026-05-20

This directory provides a per-table data dictionary for every persisted
table in the RÉCOR platform. The dictionary supplements the schema
migrations (the source of truth for types and constraints) and the
data-classification document (`docs/compliance/data-classification.md`)
by adding GDPR legal basis and retention policy per column.

## How to read these tables

Each per-table file contains the following columns:

| Column heading | Meaning |
|---|---|
| **Column** | Exact column name as it appears in Postgres |
| **Type** | Postgres type + nullability + default |
| **Constraints** | CHECK, FK, UNIQUE, or trigger constraints |
| **Classification** | Tier per `docs/compliance/data-classification.md`: Public / Internal / Confidential / PII / Sensitive-PII |
| **GDPR legal basis** | The Art. 6 basis (or exemption) for holding this column |
| **Retention** | Per `docs/compliance/data-retention.md` |

## Index of tables

| File | Tables covered | Service |
|---|---|---|
| `declarations.md` | `declarations`, `declaration_events`, `idempotency_records`, `outbox`, `outbox_dlq` | declaration |
| `verification.md` | `verification_cases`, `verification_outbox`, `verification_outbox_dlq`, `mock_bunec_persons` | verification-engine |
| `discrepancies.md` | `discrepancies`, `discrepancy_events` | declaration |
| `fiu-disclosure-log.md` | `fiu_disclosure_log` | declaration |
| `public-feedback-log.md` | `public_feedback_log` | declaration |
| `sanctions-proceedings.md` | `sanctions_proceedings`, `sanction_events` | declaration |
| `planned-tables.md` | `persons`, `person_events`, `entities`, `entity_events`, `arrangements` (planned) | person-service, entity-service |

## Change procedure

When a migration adds or alters a column:

1. Add or update the row in the relevant file here in the same PR
   (Doctrine D05 — documentation is part of the feature).
2. If the classification changes, follow the change procedure in
   `docs/compliance/data-classification.md` § Change procedure.
3. If a new PII field name is introduced, update `UUID_PII_FIELDS`
   in `packages/recor-logging/src/lib.rs` in the same PR.
4. If the retention rule changes, open an ADR; retention changes
   are load-bearing legal decisions.

## Authority hierarchy

Schema migrations → this dictionary → classification document →
GDPR procedures. When any two conflict, the migration is the ground truth
for type/constraints; the classification document is the ground truth for
PII tier; the GDPR procedures document is the ground truth for rights
exercisable against the field.
