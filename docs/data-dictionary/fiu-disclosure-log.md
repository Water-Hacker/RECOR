# Data dictionary — fiu_disclosure_log

**Service:** `services/declaration`
**Migration:** `0012_fiu_disclosure_log.sql`
**Last updated:** 2026-05-20

---

## Table `fiu_disclosure_log`

This table is the GDPR Art. 30 records-of-processing log for every
FIU-initiated access to the platform's PII. It is append-only (immutable
triggers reuse `declaration_events_refuse_mutation()`), retained forever,
and accessible to the DPO and the security-team lead only.

Every ANIF search and every R.40 / MLAT foreign-FIU access generates one
row. The row records which columns were disclosed — not the column values
themselves — so an audit can answer "what did ANIF receive" without the
table duplicating the underlying PII.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `disclosure_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) — GDPR Art. 30 records-of-processing obligation | **Forever** — audit record |
| `requesting_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — identity of the ANIF / FIU operator who issued the query | Forever |
| `anif_case_reference` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(c) — the FIU's own case reference; not publicly disclosed | Forever |
| `justification_text` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(c) — the operator's documented reason for the query; required by policy | Forever |
| `subject_kind` | `TEXT NOT NULL` | CHECK IN ('person_id','national_id','declaration_id','entity_id','full_name') | Internal | Art. 6(1)(c) | Forever |
| `subject_value` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — the search term used by the FIU; may be a person_id, full name, or national_id | Forever |
| `disclosed_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `disclosed_columns` | `JSONB NOT NULL` | — | Internal | Art. 6(1)(c) — array of column names returned in the response; no PII values | Forever |
| `resolved_declaration_id` | `UUID NULL` | — | Public | Art. 6(1)(c) — links disclosure to the declaration, if one was resolved | Forever |
| `mlat_foreign_fiu` | `TEXT NULL` | — | Confidential | Art. 6(1)(e) public task — FATF R.40 / Egmont mutual assistance | Forever |
| `mlat_egmont_request_id` | `TEXT NULL` | — | Confidential | Art. 6(1)(e) | Forever |
| `event_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) — links to the COMP-2 audit-log row for cryptographic provenance (D15) | Forever |

### Immutability

BEFORE UPDATE/DELETE/TRUNCATE triggers fire via
`declaration_events_refuse_mutation()`. REVOKE UPDATE, DELETE, TRUNCATE
from PUBLIC. The retention worker does not touch this table.

---

## References

- `services/declaration/migrations/0012_fiu_disclosure_log.sql`
- `docs/compliance/data-classification.md` (COMP-3)
- `docs/compliance/gdpr-procedures.md` (COMP-1) — Art. 30 obligation
- FATF R.24 c.24.9 — FIU timely access requirement
- FATF R.40 — international cooperation and MLAT
