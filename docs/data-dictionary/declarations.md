# Data dictionary — declaration service tables

**Service:** `services/declaration`
**Migrations:** `services/declaration/migrations/`
**Last updated:** 2026-05-20

---

## Table `declarations` (current-state projection)

Primary migration: `0001_initial.sql` L17-L33.
Extended by: `0002`, `0003`, `0004`, `0006`, `0009`, `0010`.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `declaration_id` | `UUID NOT NULL` | PK | Public | Art. 6(1)(c) statutory obligation | Forever — public register record |
| `entity_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `declarant_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — declarant is required by law to be on record | Forever under OHADA AML/CFT carve-out; GDPR Art. 17(3)(b) exemption applies |
| `declarant_role` | `TEXT NOT NULL` | CHECK IN ('self','authorised_agent','operator_assisted') | Public | Art. 6(1)(c) | Forever |
| `declaration_kind` | `TEXT NOT NULL` | CHECK IN ('incorporation','annual_renewal','change_of_control','correction','amendment') | Public | Art. 6(1)(c) | Forever |
| `effective_from` | `DATE NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `beneficial_owners` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) | Forever under OHADA AML/CFT carve-out; partial-erasure procedure in `gdpr-procedures.md` § 3 |
| `adequacy_claims` | `JSONB NULL` | — | **PII** (declarant-authored; signed) | Art. 6(1)(c) — FATF c.24.8 requires the attestation | Forever |
| `attestation` | `JSONB NOT NULL` | — | Confidential | Art. 6(1)(c) | Forever — D15 receipt-chain anchor |
| `state` | `TEXT NOT NULL` | CHECK IN ('draft','submitted','in_verification','accepted','rejected','superseded') | Public | Art. 6(1)(c) | Forever |
| `aggregate_version` | `BIGINT NOT NULL` | — | Confidential | Art. 6(1)(c) (service-internal ordering) | Forever |
| `submitted_at` | `TIMESTAMPTZ NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `receipt_hash_hex` | `TEXT NOT NULL` | CHECK (length = 64 hex chars) | Confidential | Art. 6(1)(c) | Forever — D15 |
| `correlation_id` | `UUID NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |
| `updated_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW(), updated by trigger | Internal | Art. 6(1)(c) | Forever |
| `verification_state` | `TEXT NOT NULL` | DEFAULT 'not_verified'; CHECK IN ('not_verified','pending','in_verification','accepted','rejected') | Public | Art. 6(1)(c) | Forever |
| `verification_lane` | `TEXT NULL` | CHECK IN ('green','yellow','red') | Public | Art. 6(1)(c) | Forever |
| `verification_case_id` | `UUID NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `verified_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) | Forever |
| `supersedes_declaration_id` | `UUID NULL` | — | Public | Art. 6(1)(c) | Forever |
| `superseded_by_declaration_id` | `UUID NULL` | — | Public | Art. 6(1)(c) | Forever |
| `superseded_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) | Forever |
| `metadata_notes` | `TEXT NULL` | — | Confidential | Art. 6(1)(c) — annotation at correction time | Forever |
| `amended_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) | Forever |
| `corrected_at` | `TIMESTAMPTZ NULL` | — | Public | Art. 6(1)(c) | Forever |
| `last_event_observed_at` | `TIMESTAMPTZ NULL` | — | Internal | Art. 6(1)(c) — staleness watcher trigger | Forever |
| `nonce_hex` | `TEXT NULL` | UNIQUE; 32-byte hex — migration `0008` | Confidential | Art. 6(1)(c) — replay prevention | Forever |

### Notes on `beneficial_owners` JSONB

The JSONB payload carries an array of `BeneficialOwnerClaim` objects.
Each object may include `cascade_tier`, `control_basis`,
`cascade_tier_b_ruled_out_evidence`, `is_nominee`, `nominator_person_id`
(all added by migration `0009` for FATF compliance per ADR-0010).
Every `person_id` key inside the payload is PII; the structural keys
(`ownership_basis_points`, `cascade_tier` enum values, etc.) are not.
The whole JSONB column is handled as PII for log-redaction purposes.

### Notes on `adequacy_claims` JSONB

Added by migration `0009`. Shape:
`{ "adequate": bool, "accurate": bool, "up_to_date_as_of": ISO8601, "legal_basis": string }`.
The declarant cryptographically attests this block alongside
`beneficial_owners`. Classification: PII because it is
declarant-authored and signed; exposure reveals the declarant's legal
assertions.

---

## Table `declaration_events` (append-only event log)

Primary migration: `0001_initial.sql` L61-L71.
Immutability: `0007_audit_log_immutability.sql` — BEFORE
UPDATE/DELETE/TRUNCATE triggers + REVOKE from PUBLIC.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `seq_id` | `BIGSERIAL NOT NULL` | PK | Internal | Art. 6(1)(c) | **Forever** — immutable log; triggers refuse DELETE |
| `declaration_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `aggregate_version` | `BIGINT NOT NULL` | CHECK >= 1 | Confidential | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `event_payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) + Art. 17(3)(b) exemption; payload carries `declarant_principal` and `beneficial_owners` | **Forever** — immutable; OHADA AML/CFT exemption prevents erasure |
| `event_time` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |
| `correlation_id` | `UUID NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `causation_id` | `UUID NULL` | — | Internal | Art. 6(1)(c) | Forever |

> The `event_payload` column is retained forever and cannot be erased on
> a GDPR Art. 17 request. The platform's documented position is the
> OHADA AML/CFT statutory carve-out (see `gdpr-procedures.md` § 3).
> Partial erasure of the *projection* (`declarations.beneficial_owners`)
> is possible; the event log row cannot be mutated.

---

## Table `idempotency_records`

Primary migration: `0001_initial.sql` L86-L96.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `idempotency_key` | `TEXT NOT NULL` | PK | Internal | Art. 6(1)(c) | TTL 24h (`expires_at > NOW()` predicate) |
| `declarant_principal` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) | TTL 24h |
| `request_hash` | `TEXT NOT NULL` | CHECK (length = 64) | Confidential | Art. 6(1)(c) | TTL 24h |
| `response_status` | `SMALLINT NOT NULL` | CHECK BETWEEN 100 AND 599 | Internal | Art. 6(1)(c) | TTL 24h |
| `response_body` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) — replay body inherits classification of underlying content | TTL 24h |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | TTL 24h |
| `expires_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Self-expiring; stale rows filtered by query predicate |

---

## Table `outbox`

Primary migration: `0001_initial.sql` L102-L116.
Pruned 30 days post-`dispatched_at` by the retention worker.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `id` | `UUID NOT NULL` | PK DEFAULT gen_random_uuid() | Internal | Art. 6(1)(c) | 30 days after `dispatched_at`; un-dispatched rows exempt |
| `event_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) | 30 days after dispatch |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | 30 days after dispatch |
| `event_version` | `INTEGER NOT NULL` | CHECK >= 1 | Public | Art. 6(1)(c) | 30 days after dispatch |
| `aggregate_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | 30 days after dispatch |
| `aggregate_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | 30 days after dispatch |
| `partition_key` | `TEXT NOT NULL` | — | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) | 30 days after dispatch |
| `headers` | `JSONB NOT NULL` | DEFAULT '{}' | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `dispatched_at` | `TIMESTAMPTZ NULL` | — | Internal | Art. 6(1)(c) | NULL rows never pruned |
| `dispatch_attempts` | `INT NOT NULL` | DEFAULT 0; CHECK >= 0 | Internal | Art. 6(1)(c) | 30 days after dispatch |
| `last_error` | `TEXT NULL` | — | Internal | Art. 6(1)(c) | 30 days after dispatch |

---

## Table `outbox_dlq`

Primary migration: `0005_add_outbox_dlq.sql` L43-L65.
Retained **forever** for forensic use; the retention worker never
touches this table.

| Column | Type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `id` | `UUID NOT NULL` | PK (preserved from `outbox.id`) | Internal | Art. 6(1)(c) | **Forever** — forensic |
| `event_id` | `UUID NOT NULL` | UNIQUE | Public | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `event_version` | `INTEGER NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `aggregate_type` | `TEXT NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `aggregate_id` | `UUID NOT NULL` | — | Public | Art. 6(1)(c) | Forever |
| `partition_key` | `TEXT NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `payload` | `JSONB NOT NULL` | — | **PII** | Art. 6(1)(c) | Forever — DLQ rows are forensic evidence; PII inside is protected by service-role-only access and audit log |
| `headers` | `JSONB NOT NULL` | DEFAULT '{}' | Internal | Art. 6(1)(c) | Forever |
| `created_at` | `TIMESTAMPTZ NOT NULL` | Preserved from live row | Internal | Art. 6(1)(c) | Forever |
| `dead_lettered_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |
| `dispatch_attempts` | `INTEGER NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |
| `last_error` | `TEXT NULL` | — | Internal | Art. 6(1)(c) | Forever; operator-only surface gated by `enforce_admin` |

---

## References

- `services/declaration/migrations/` — authoritative schema
- `docs/compliance/data-classification.md` (COMP-3) — PII tier rationale
- `docs/compliance/data-retention.md` (COMP-2) — retention enforcement
- `docs/compliance/gdpr-procedures.md` (COMP-1) — rights exercisable against PII columns
- `docs/adr/0001-event-sourcing-declaration-aggregate.md` — why the event log is append-only
- `docs/adr/0010-fatf-bo-cascade-and-adequacy.md` — FATF cascade fields in `beneficial_owners`
