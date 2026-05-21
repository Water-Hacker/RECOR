# Data dictionary — verification-engine tables

**Service:** `services/verification-engine`
**Migrations:** `services/verification-engine/migrations/`
**Last updated:** 2026-05-20

---

## Table `verification_cases` (append-only adjudication record)

Primary migration: `0001_initial.sql` L8-L21.
Immutability: `0003_audit_log_immutability.sql` — BEFORE
UPDATE/DELETE/TRUNCATE triggers + REVOKE from PUBLIC.
ADR reference: ADR-0002 (fusion math requires `case_payload` to be
byte-identical post-adjudication).

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `case_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) statutory obligation | **Forever** — immutable adjudication record |
| `declaration_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) | Forever |
| `entity_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `declarant_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) + Art. 17(3)(b) exemption | Forever — OHADA AML/CFT carve-out; replicated from declaration for verification audit trail |
| `lane` | `TEXT NOT NULL` | CHECK IN ('green','yellow','red') | Public | Art. 6(1)(c) | Forever |
| `authenticity_belief` | `DOUBLE PRECISION NOT NULL` | — | Confidential | Art. 6(1)(c) — internal scoring; consumer-access surface gates access per V1 P11 | Forever |
| `authenticity_plausibility` | `DOUBLE PRECISION NOT NULL` | — | Confidential | Art. 6(1)(c) | Forever |
| `risk_belief` | `DOUBLE PRECISION NOT NULL` | — | Confidential | Art. 6(1)(c) | Forever |
| `case_payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) + Art. 17(3)(b) exemption | **Forever** — immutable; embeds declaration PII; ADR-0002 audit requirement |
| `created_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `completed_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `total_duration_ms` | `BIGINT NOT NULL` | CHECK >= 0 | Internal | Art. 6(1)(c) — performance metric | Forever |

### Notes on `case_payload`

The payload carries the full `DeclarationSnapshot` (including PII fields)
plus every stage's `evidence` JSON object. It is retained in
byte-identical form because ADR-0002 requires the Dempster-Shafer
fusion math to be deterministically replayable by a third-party auditor.
The GDPR Art. 17 erasure request against this column is refused under the
OHADA AML/CFT statutory carve-out; the platform's partial-erasure
procedure affects the declaration-service *projection*, not this record.

---

## Table `mock_bunec_persons` (dev/test fixture only)

Primary migration: `0001_initial.sql` L33-L38.
**This table carries no production data.** It is replaced by the real
BUNEC adapter under R-VER-1. The classifications below apply to the
data shape, not to the synthetic content.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `person_id` | `UUID NOT NULL` | PK | **PII** | Art. 6(1)(c) (data shape, not actual production data) | Dev/test fixture only — no production retention rule |
| `canonical_full_name` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) | Dev/test fixture only |
| `nationality` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) | Dev/test fixture only |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Dev/test fixture only |

> Production fixtures must remain synthetic; real BUNEC data is never
> loaded into this table. The table is dropped and replaced when
> R-VER-1 ships.

---

## Table `verification_outbox`

Primary migration: `0001_initial.sql` L41-L53.
Pruned 30 days post-`dispatched_at` by the verification-engine
retention worker (`infrastructure/retention.rs`).

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `id` | `UUID NOT NULL` | PK DEFAULT gen_random_uuid() | Internal | Art. 6(1)(c) | 30 days after `dispatched_at`; un-dispatched rows exempt |
| `event_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) | 30 days after dispatch |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | 30 days after dispatch |
| `event_version` | `INTEGER NOT NULL` | CHECK >= 1 | Public | Art. 6(1)(c) | 30 days after dispatch |
| `aggregate_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | 30 days after dispatch |
| `partition_key` | `TEXT NOT NULL` | — | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) — writeback envelope embeds `declarant_principal` | 30 days after dispatch |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `dispatched_at` | `TIMESTAMPTZ NULL` | — | Internal | Art. 6(1)(c) | NULL rows never pruned |
| `dispatch_attempts` | `INT NOT NULL` | DEFAULT 0 | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `last_error` | `TEXT NULL` | — | Internal | Art. 6(1)(c) | 30 days after dispatch |

---

## Table `verification_outbox_dlq`

Primary migration: `0002_add_verification_outbox_dlq.sql` L17-L30.
Retained **forever** for forensic use.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `id` | `UUID NOT NULL` | PK | Internal | Art. 6(1)(c) | **Forever** — forensic |
| `event_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `event_version` | `INTEGER NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `aggregate_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `partition_key` | `TEXT NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) | Forever — forensic; service-role-only access |
| `created_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `dead_lettered_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |
| `dispatch_attempts` | `INTEGER NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `last_error` | `TEXT NULL` | — | Internal | Art. 6(1)(c) | Forever |

---

## References

- `services/verification-engine/migrations/` — authoritative schema
- `docs/compliance/data-classification.md` (COMP-3)
- `docs/compliance/data-retention.md` (COMP-2)
- `docs/adr/0002-dempster-shafer-fusion.md` — why `case_payload` must be byte-identical
- `docs/adr/0014-stage7-cross-source-decision-rules.md`
- Architecture V4 P14 § Verification Engine
