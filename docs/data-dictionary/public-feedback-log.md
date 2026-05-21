# Data dictionary — public_feedback_log

**Service:** `services/declaration`
**Migration:** `0013_public_feedback_log.sql`
**Last updated:** 2026-05-20

---

## Table `public_feedback_log`

Stores public discrepancy reports submitted via `POST /v1/public-feedback`.
The submitter is pseudonymous (no required authenticated identity). The
table is append-only (immutable triggers) and retained for 90 days from
`submitted_at` for operational triage, after which rows are eligible for
pruning by the back-office workflow (not the outbox retention worker).

At least one of `declaration_id` or `entity_id` must be non-NULL
(CHECK constraint `public_feedback_target_present`).

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `feedback_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(e) public task — FATF R.24 Guidance §3.5 public feedback obligation | 90 days from `submitted_at` |
| `declaration_id` | `UUID NULL` | — | Public | Art. 6(1)(e) | 90 days |
| `entity_id` | `UUID NULL` | — | Public | Art. 6(1)(e) | 90 days |
| `submitter_contact` | `TEXT NULL` | — | **PII** | Art. 6(1)(a) consent (submitter opts in by providing contact); nullable because anonymous submission is permitted | 90 days; subject to Art. 17 erasure on request for the contact field |
| `captcha_token_hash` | `TEXT NULL` | — | Confidential | Art. 6(1)(e) — BLAKE3 digest of the validated CAPTCHA token; proves the rate-limit gate fired without retaining the raw token (D18) | 90 days |
| `submitter_ip_hash` | `TEXT NULL` | — | Confidential | Art. 6(1)(f) legitimate interest — fraud and rate-limit enforcement; IP address is hashed, not stored in clear | 90 days; hashed form only |
| `description` | `TEXT NOT NULL` | — | Confidential | Art. 6(1)(e) — may incidentally contain PII authored by the submitter; treated as Confidential for the whole field; audited for incidental PII before the 90-day window begins | 90 days |
| `evidence_url` | `TEXT NULL` | — | Internal | Art. 6(1)(e) | 90 days |
| `triage_priority` | `TEXT NOT NULL` | CHECK IN ('low','normal','high'); DEFAULT 'normal' | Internal | Art. 6(1)(e) | 90 days |
| `state` | `TEXT NOT NULL` | CHECK IN ('submitted','triaged','resolved','dismissed'); DEFAULT 'submitted' | Public | Art. 6(1)(e) | 90 days |
| `submitted_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(e) | 90 days |
| `resolved_at` | `TIMESTAMPTZ NULL` | — | Internal | Art. 6(1)(e) | 90 days |
| `resolution_notes` | `TEXT NULL` | — | Confidential | Art. 6(1)(e) | 90 days |

### Notes on `description` PII handling

The `description` field is free text from an unauthenticated or
pseudonymous submitter. It may contain names, identity references, or
other PII about third parties. The platform's pre-persistence audit step
(operational process; not schema-enforced) reviews the field before the
row enters the 90-day window. PII detected by the audit is not persisted;
the submission is returned to the submitter with guidance to omit personal
data. Only the entity reference and the factual discrepancy description
are required.

### Notes on `submitter_ip_hash`

The submitter's IP address is hashed (BLAKE3) by the handler before
persistence and the raw IP is never written to the database. The hash
supports rate-limiting and fraud analysis (identifying mass-report
campaigns) without retaining personally-identifying network data.

### Immutability

BEFORE UPDATE/DELETE/TRUNCATE triggers fire via
`declaration_events_refuse_mutation()`. REVOKE UPDATE, DELETE, TRUNCATE
from PUBLIC. Note: the 90-day pruning is a back-office operation (DELETE
authorised for the back-office role after the retention window); the
standard retention worker does not touch this table.

---

## References

- `services/declaration/migrations/0013_public_feedback_log.sql`
- `docs/compliance/data-classification.md` (COMP-3)
- FATF R.24 Guidance §3.5 — public feedback obligation
- 6AMLD Art. 10 — discrepancy reporting
- Open Ownership Principle 5.5 — consequences for non-compliance
