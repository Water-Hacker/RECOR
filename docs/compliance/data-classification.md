# Data classification — RÉCOR platform

**Ticket:** COMP-3
**Status:** Draft — engineering complete, **pending AML/CFT counsel sign-off** on the PII / Sensitive-PII delineation for Cameroonian natural-person fields.
**Owners:** RÉCOR security-engineering (engineering), [counsel name TBD] (legal).
**Last updated:** 2026-05-12.

This document classifies every column in every persisted table across
the two shipped services — `services/declaration` and
`services/verification-engine` — under a five-tier scheme: **Public**,
**Internal**, **Confidential**, **PII**, **Sensitive-PII**. The
classification governs:

- whether a field may appear in cached responses, public APIs, or logs;
- whether the field is subject to the data-subject rights documented at
  `docs/compliance/gdpr-procedures.md`;
- the retention rule that applies (`docs/compliance/data-retention.md`);
- the redaction obligations enforced by the OPS-2 layer in
  `packages/recor-logging/src/lib.rs`;
- the threat-model row in `docs/security/threat-model.md` (§ Per-component
  STRIDE → Database) that cites this document as its companion artefact.

The migration files are the source of truth for the schema. Every row
in the inventory tables below links to the line range of the column
definition. The links resolve as GitHub web-view anchors on the `main`
branch.

## Authoritative documents

- **Architecture V1 P2** — the doctrines, in particular D14 (fail-closed),
  D15 (cryptographic provenance), D17 (zero trust), D18 (no secrets in
  code/logs).
- **Architecture V4 P14** — canonical data model for declaration and
  verification.
- **`docs/compliance/gdpr-procedures.md` (COMP-1)** — operational
  procedures for the six GDPR data-subject rights.
- **`docs/compliance/data-retention.md` (COMP-2)** — retention rule per
  table; this document and the retention doc together describe the
  per-column treatment of PII.
- **`docs/security/threat-model.md` (DOC-4)** — STRIDE row § Per-component
  → Database cites this document for the per-column treatment.

## The classification scheme

Five tiers; mutually exclusive; the most-restrictive applicable tier
wins when a field arguably qualifies for two.

| Tier | Definition | Handling rule |
|---|---|---|
| **Public** | Information that is statutorily public under Cameroon's beneficial-ownership law (entity identity, declaration lifecycle state, the fact that a declaration exists). May appear in cached responses and public APIs. | May appear in cached responses, public APIs, structured logs. No redaction. Subject to retention rules but not to data-subject access rights (public-record exemption). |
| **Internal** | Operator surface only: queue mechanics, dispatch counters, dead-letter envelopes, idempotency keys, retry metadata. Not user-facing; not part of the registry's published record. | Operator surface only; admin allowlist gates exposure (`enforce_admin` in `services/declaration/src/api/dlq.rs`). Not redacted in operator logs because operators are the authorised audience. NOT exposed via consumer or public APIs. |
| **Confidential** | Service-internal state with no business meaning to other services: receipt hashes, partition keys, dispatch attempt counts, aggregate version numbers, correlation IDs. Service-role-only credential required to read. | Service-role-only access; not replicated cross-service without explicit need; partial-hex display only in logs (e.g. the first 8 chars of a `receipt_hash_hex` are acceptable; the full value is not). |
| **PII** | Personally-identifying information about a natural person: SPIFFE-shaped or other identity-bearing principals, declarant identifiers, beneficial-owner person identifiers carried inside structured payloads. | Redacted in logs by the OPS-2 `RedactingLayer` (`packages/recor-logging/src/lib.rs`). Subject to GDPR / OHADA data-subject rights — see `docs/compliance/gdpr-procedures.md`. **Never** placed in URL path or query parameters (URLs end up in proxy logs, browser history, referrers). Replicable cross-service only via authenticated, audited channels. |
| **Sensitive-PII** | A subset of PII that warrants field-level protection beyond logging redaction: biometric references, primary identity-document numbers, government-issued IDs. **No Sensitive-PII column ships in the current schema.** Future fields under R-DECL-4 / IDENTITY-1 carry this classification. | Field-level encryption REQUIRED (placeholder ticket `R-ENC-FIELD-LEVEL`, not yet filed). Access audited per-row to an immutable audit channel separate from `declaration_events`. Subject to the strictest GDPR / OHADA controls. |

### Why "Public" exists at all

The beneficial-ownership register is, by law, a partially-public
instrument. Entity identity (the `entity_id`) and the fact that a
declaration was filed against an entity on a given effective date are
matters of public record under Cameroon's transparency framework. The
register's downstream consumer-access design (Architecture V1 P11
§ Consumer Access) implements the CJEU's Case C-37/20 ruling, which
constrains which natural-person fields may be exposed publicly — but
the corporate-identity fields remain public by statute.

### Why some apparent identifiers are not PII

UUIDs in fields such as `entity_id`, `declaration_id`, `correlation_id`,
`event_id`, and `case_id` are not personally identifying. They are
synthetic identifiers minted by the platform; they have no out-of-band
linkage to a natural person. The OPS-2 redaction layer therefore
explicitly does **not** mask these UUIDs in logs (see
`packages/recor-logging/src/lib.rs` line range
[L68-L76](../../packages/recor-logging/src/lib.rs#L68-L76)). Only
UUIDs in `person_id`, `principal`, `declarant_principal`, and
`subject` are treated as PII.

## Service: `services/declaration`

### Table `declarations` (current-state projection)

Defined in
[`services/declaration/migrations/0001_initial.sql#L17-L33`](../../services/declaration/migrations/0001_initial.sql#L17-L33).
Extended by migrations 0002, 0003, 0004, 0006.

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `declaration_id` | UUID PK | Public | [0001#L18](../../services/declaration/migrations/0001_initial.sql#L18) | Synthetic identifier; the declaration's external handle. |
| `entity_id` | UUID NOT NULL | Public | [0001#L19](../../services/declaration/migrations/0001_initial.sql#L19) | Identifies a legal entity; entities are public-register data. |
| `declarant_principal` | TEXT NOT NULL | **PII** | [0001#L20](../../services/declaration/migrations/0001_initial.sql#L20) | SPIFFE URI or OIDC subject of the natural person filing the declaration. Redacted in logs by `UUID_PII_FIELDS` + SPIFFE-path MAC at `packages/recor-logging/src/lib.rs#L71-L76`. Subject to GDPR Art. 15 access via `GET /v1/declarations/by-principal`. |
| `declarant_role` | TEXT NOT NULL | Public | [0001#L21](../../services/declaration/migrations/0001_initial.sql#L21) | Enum {self, authorised_agent, operator_assisted}; carries no identity. |
| `declaration_kind` | TEXT NOT NULL | Public | [0001#L22](../../services/declaration/migrations/0001_initial.sql#L22) | Enum {incorporation, annual_renewal, change_of_control, correction, amendment}. |
| `effective_from` | DATE NOT NULL | Public | [0001#L23](../../services/declaration/migrations/0001_initial.sql#L23) | The date the declaration takes effect. Statutorily public. |
| `beneficial_owners` | JSONB NOT NULL | **PII** | [0001#L24](../../services/declaration/migrations/0001_initial.sql#L24) | Structured payload carrying one or more `person_id` UUIDs and ownership basis points. The `person_id` keys inside the payload are PII; the structural keys (`basis_points`, etc.) are not. Treat the entire JSONB as PII for handling. Subject to GDPR rights. |
| `attestation` | JSONB NOT NULL | Confidential | [0001#L25](../../services/declaration/migrations/0001_initial.sql#L25) | Ed25519 attestation envelope: signature hex, public key hex, canonical-form digest. Service-role-only; partial-hex display in logs only. D15 receipt-chain anchor. |
| `state` | TEXT NOT NULL | Public | [0001#L26](../../services/declaration/migrations/0001_initial.sql#L26) | Aggregate lifecycle state {draft, submitted, in_verification, accepted, rejected, superseded}. The lifecycle of a declaration is public-record material. |
| `aggregate_version` | BIGINT NOT NULL | Confidential | [0001#L27](../../services/declaration/migrations/0001_initial.sql#L27) | Event-source version counter. Service-internal; meaningless across service boundaries. |
| `submitted_at` | TIMESTAMPTZ NOT NULL | Public | [0001#L28](../../services/declaration/migrations/0001_initial.sql#L28) | When the declaration was filed. Part of the public lifecycle record. |
| `receipt_hash_hex` | TEXT NOT NULL CHECK len=64 | Confidential | [0001#L29](../../services/declaration/migrations/0001_initial.sql#L29) | BLAKE3-256 over the canonical receipt bytes. Service-role-only; partial-prefix display in logs (the redaction layer truncates `receipt_hash_hex` via `RECEIPT_HASH_FIELD` in `packages/recor-logging/src/lib.rs#L78-L79`). |
| `correlation_id` | UUID NOT NULL | Internal | [0001#L30](../../services/declaration/migrations/0001_initial.sql#L30) | Tracing correlation. Not PII (see § "Why some apparent identifiers are not PII"). Operator-surface only by convention; not exposed to public consumers. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L31](../../services/declaration/migrations/0001_initial.sql#L31) | Row-creation timestamp; operator-surface. `submitted_at` is the public business-meaning timestamp. |
| `updated_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L32](../../services/declaration/migrations/0001_initial.sql#L32) | Auto-maintained by trigger `trg_declarations_updated_at`; operator-surface. |
| `verification_state` | TEXT NOT NULL DEFAULT 'not_verified' | Public | [0002#L14-L16](../../services/declaration/migrations/0002_add_verification_state.sql#L14-L16) | Downstream verification lifecycle {not_verified, pending, in_verification, accepted, rejected}. Public-record material. |
| `verification_lane` | TEXT CHECK lane | Public | [0003#L21-L23](../../services/declaration/migrations/0003_add_verification_outcome_columns.sql#L21-L23) | The fusion engine's lane decision {green, yellow, red}. Public for green; the platform's consumer-access surface gates yellow / red exposure per V1 P11. |
| `verification_case_id` | UUID | Internal | [0003#L25-L26](../../services/declaration/migrations/0003_add_verification_outcome_columns.sql#L25-L26) | FK-shaped reference to `verification_cases.case_id` in the V-engine. Operator surface (case detail is internal). |
| `verified_at` | TIMESTAMPTZ | Public | [0003#L28-L29](../../services/declaration/migrations/0003_add_verification_outcome_columns.sql#L28-L29) | Timestamp of the lane decision; public-record material alongside `verification_state`. |
| `supersedes_declaration_id` | UUID | Public | [0004#L36-L37](../../services/declaration/migrations/0004_add_supersede_chain.sql#L36-L37) | Back-link to the declaration this one replaces. Lifecycle relationship; public. |
| `superseded_by_declaration_id` | UUID | Public | [0004#L39-L40](../../services/declaration/migrations/0004_add_supersede_chain.sql#L39-L40) | Forward-link to the declaration that replaced this one. Public. |
| `superseded_at` | TIMESTAMPTZ | Public | [0004#L42-L43](../../services/declaration/migrations/0004_add_supersede_chain.sql#L42-L43) | Timestamp of supersession. Public. |
| `metadata_notes` | TEXT | Confidential | [0006#L39-L40](../../services/declaration/migrations/0006_add_correction_columns.sql#L39-L40) | Free-form declarant-supplied annotation attached at correction time. May incidentally contain references to supporting documents or PII fragments; treat as Confidential for the whole field. NOT exposed via consumer APIs. |
| `amended_at` | TIMESTAMPTZ | Public | [0006#L42-L43](../../services/declaration/migrations/0006_add_correction_columns.sql#L42-L43) | Last-amendment timestamp; public lifecycle event. |
| `corrected_at` | TIMESTAMPTZ | Public | [0006#L45-L46](../../services/declaration/migrations/0006_add_correction_columns.sql#L45-L46) | Last-correction timestamp; public lifecycle event. |

### Table `declaration_events` (append-only event log)

Defined in
[`services/declaration/migrations/0001_initial.sql#L61-L71`](../../services/declaration/migrations/0001_initial.sql#L61-L71).
Immutability enforced by triggers in
[`0007_audit_log_immutability.sql`](../../services/declaration/migrations/0007_audit_log_immutability.sql).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `seq_id` | BIGSERIAL PK | Internal | [0001#L62](../../services/declaration/migrations/0001_initial.sql#L62) | Append-order sequence; operator-surface only. |
| `declaration_id` | UUID NOT NULL | Public | [0001#L63](../../services/declaration/migrations/0001_initial.sql#L63) | Same as `declarations.declaration_id`. |
| `aggregate_version` | BIGINT NOT NULL CHECK >=1 | Confidential | [0001#L64](../../services/declaration/migrations/0001_initial.sql#L64) | Per-aggregate event version. Service-internal. |
| `event_type` | TEXT NOT NULL | Public | [0001#L65](../../services/declaration/migrations/0001_initial.sql#L65) | Event-name string (e.g. `declaration.submitted.v1`). Public registry mechanics. |
| `event_payload` | JSONB NOT NULL | **PII** | [0001#L66](../../services/declaration/migrations/0001_initial.sql#L66) | The event's payload; for `declaration.submitted.v1` carries the full beneficial-owner list, declarant principal, and attestation. Treat as PII for the whole field. D15 receipt-chain payload. |
| `event_time` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L67](../../services/declaration/migrations/0001_initial.sql#L67) | Server-set wallclock; operator-surface. |
| `correlation_id` | UUID NOT NULL | Internal | [0001#L68](../../services/declaration/migrations/0001_initial.sql#L68) | Tracing correlation. |
| `causation_id` | UUID NULL | Internal | [0001#L69](../../services/declaration/migrations/0001_initial.sql#L69) | Causal predecessor event id; tracing. |

### Table `idempotency_records` (idempotency cache)

Defined in
[`services/declaration/migrations/0001_initial.sql#L86-L96`](../../services/declaration/migrations/0001_initial.sql#L86-L96).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `idempotency_key` | TEXT PK | Internal | [0001#L87](../../services/declaration/migrations/0001_initial.sql#L87) | Client-supplied opaque token; operator-surface. Not personally identifying by itself; treat as Internal to avoid leakage of the request-routing surface. |
| `declarant_principal` | TEXT NOT NULL | **PII** | [0001#L88](../../services/declaration/migrations/0001_initial.sql#L88) | Same field name as `declarations.declarant_principal`; same redaction by OPS-2. Pair-key with `idempotency_key` for replay scoping. |
| `request_hash` | TEXT NOT NULL CHECK len=64 | Confidential | [0001#L89](../../services/declaration/migrations/0001_initial.sql#L89) | BLAKE3 over the canonical request bytes; service-role-only. |
| `response_status` | SMALLINT NOT NULL CHECK 100..599 | Internal | [0001#L90](../../services/declaration/migrations/0001_initial.sql#L90) | HTTP status of the replayable response. |
| `response_body` | JSONB NOT NULL | **PII** | [0001#L91](../../services/declaration/migrations/0001_initial.sql#L91) | The exact previous response body, which for `POST /v1/declarations` contains the declaration receipt including PII payload fields. Inherits classification of its underlying content. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L92](../../services/declaration/migrations/0001_initial.sql#L92) | Row-creation. |
| `expires_at` | TIMESTAMPTZ NOT NULL | Internal | [0001#L93](../../services/declaration/migrations/0001_initial.sql#L93) | TTL marker; the application filters expired rows via `expires_at > NOW()` predicate (see `data-retention.md`). |

### Table `outbox` (event outbox)

Defined in
[`services/declaration/migrations/0001_initial.sql#L102-L116`](../../services/declaration/migrations/0001_initial.sql#L102-L116).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `id` | UUID PK DEFAULT gen_random_uuid() | Internal | [0001#L103](../../services/declaration/migrations/0001_initial.sql#L103) | Outbox row identity; operator-surface. |
| `event_id` | UUID NOT NULL UNIQUE | Public | [0001#L104](../../services/declaration/migrations/0001_initial.sql#L104) | The event identity carried to consumers; not PII (synthetic). |
| `event_type` | TEXT NOT NULL | Public | [0001#L105](../../services/declaration/migrations/0001_initial.sql#L105) | Same shape as `declaration_events.event_type`. |
| `event_version` | INTEGER NOT NULL CHECK >=1 | Public | [0001#L106](../../services/declaration/migrations/0001_initial.sql#L106) | Schema version of the event. |
| `aggregate_type` | TEXT NOT NULL | Public | [0001#L107](../../services/declaration/migrations/0001_initial.sql#L107) | Aggregate-class name (e.g. `declaration`). |
| `aggregate_id` | UUID NOT NULL | Public | [0001#L108](../../services/declaration/migrations/0001_initial.sql#L108) | The aggregate's identity (same as `declaration_id` for declaration events). |
| `partition_key` | TEXT NOT NULL | Internal | [0001#L109](../../services/declaration/migrations/0001_initial.sql#L109) | Kafka / message-broker partition routing key; operator-surface. |
| `payload` | JSONB NOT NULL | **PII** | [0001#L110](../../services/declaration/migrations/0001_initial.sql#L110) | The same shape as `declaration_events.event_payload`; treat as PII. |
| `headers` | JSONB NOT NULL DEFAULT '{}'::JSONB | Internal | [0001#L111](../../services/declaration/migrations/0001_initial.sql#L111) | Transport headers (correlation, trace context); operator-surface. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L112](../../services/declaration/migrations/0001_initial.sql#L112) | Row creation. |
| `dispatched_at` | TIMESTAMPTZ NULL | Internal | [0001#L113](../../services/declaration/migrations/0001_initial.sql#L113) | NULL until the relay has shipped the row; gating column for retention pruning. |
| `dispatch_attempts` | INT NOT NULL DEFAULT 0 CHECK >=0 | Internal | [0001#L114](../../services/declaration/migrations/0001_initial.sql#L114) | Retry counter; operator-surface. |
| `last_error` | TEXT NULL | Internal | [0001#L115](../../services/declaration/migrations/0001_initial.sql#L115) | Last failure reason; operator-surface. |

### Table `outbox_dlq` (outbox dead-letter queue)

Defined in
[`services/declaration/migrations/0005_add_outbox_dlq.sql#L43-L65`](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L43-L65).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `id` | UUID PK | Internal | [0005#L47](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L47) | Preserved from the original `outbox.id`. |
| `event_id` | UUID NOT NULL UNIQUE | Public | [0005#L48](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L48) | Same event identity carried by the live outbox. |
| `event_type` | TEXT NOT NULL | Public | [0005#L49](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L49) | |
| `event_version` | INTEGER NOT NULL | Public | [0005#L50](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L50) | |
| `aggregate_type` | TEXT NOT NULL | Public | [0005#L51](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L51) | |
| `aggregate_id` | UUID NOT NULL | Public | [0005#L52](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L52) | |
| `partition_key` | TEXT NOT NULL | Internal | [0005#L53](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L53) | |
| `payload` | JSONB NOT NULL | **PII** | [0005#L54](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L54) | Same shape as `outbox.payload`. |
| `headers` | JSONB NOT NULL DEFAULT '{}'::JSONB | Internal | [0005#L55](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L55) | |
| `created_at` | TIMESTAMPTZ NOT NULL | Internal | [0005#L58](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L58) | Preserved from the live row. |
| `dead_lettered_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0005#L59](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L59) | Move-to-DLQ wallclock. |
| `dispatch_attempts` | INTEGER NOT NULL | Internal | [0005#L60](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L60) | |
| `last_error` | TEXT NULL | Internal | [0005#L64](../../services/declaration/migrations/0005_add_outbox_dlq.sql#L64) | Final attempt's error message. May incidentally surface request-payload fragments in panicked Rust errors; operator-surface only and gated by `enforce_admin`. |

## Service: `services/verification-engine`

### Table `verification_cases` (append-only adjudication record)

Defined in
[`services/verification-engine/migrations/0001_initial.sql#L8-L21`](../../services/verification-engine/migrations/0001_initial.sql#L8-L21).
Immutability enforced by triggers in
[`0003_audit_log_immutability.sql`](../../services/verification-engine/migrations/0003_audit_log_immutability.sql).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `case_id` | UUID PK | Public | [0001#L9](../../services/verification-engine/migrations/0001_initial.sql#L9) | Adjudication-record identity; synthetic. |
| `declaration_id` | UUID NOT NULL UNIQUE | Public | [0001#L10](../../services/verification-engine/migrations/0001_initial.sql#L10) | Back-link to `declarations.declaration_id`. |
| `entity_id` | UUID NOT NULL | Public | [0001#L11](../../services/verification-engine/migrations/0001_initial.sql#L11) | |
| `declarant_principal` | TEXT NOT NULL | **PII** | [0001#L12](../../services/verification-engine/migrations/0001_initial.sql#L12) | Replicated from the declaration; same redaction obligations as the source. Subject to GDPR access via the declaration's `GET /v1/declarations/by-principal` (the V-engine does not expose a per-principal lookup of its own). |
| `lane` | TEXT NOT NULL CHECK lane | Public | [0001#L13](../../services/verification-engine/migrations/0001_initial.sql#L13) | {green, yellow, red}. |
| `authenticity_belief` | DOUBLE PRECISION NOT NULL | Confidential | [0001#L14](../../services/verification-engine/migrations/0001_initial.sql#L14) | Dempster-Shafer belief mass; service-internal scoring. Exposed only via consumer-access surfaces gated per V1 P11. |
| `authenticity_plausibility` | DOUBLE PRECISION NOT NULL | Confidential | [0001#L15](../../services/verification-engine/migrations/0001_initial.sql#L15) | Plausibility mass; service-internal. |
| `risk_belief` | DOUBLE PRECISION NOT NULL | Confidential | [0001#L16](../../services/verification-engine/migrations/0001_initial.sql#L16) | Risk belief mass; service-internal. |
| `case_payload` | JSONB NOT NULL | **PII** | [0001#L17](../../services/verification-engine/migrations/0001_initial.sql#L17) | Full case envelope: every stage's input + BPA. Treat as PII because it embeds the declaration payload's PII fields. ADR-002 requires this column to remain byte-identical post-adjudication (auditability of the fusion math). |
| `created_at` | TIMESTAMPTZ NOT NULL | Internal | [0001#L18](../../services/verification-engine/migrations/0001_initial.sql#L18) | Adjudication start time. |
| `completed_at` | TIMESTAMPTZ NOT NULL | Internal | [0001#L19](../../services/verification-engine/migrations/0001_initial.sql#L19) | Adjudication end time. |
| `total_duration_ms` | BIGINT NOT NULL CHECK >=0 | Internal | [0001#L20](../../services/verification-engine/migrations/0001_initial.sql#L20) | Pipeline duration; operator-surface metric. |

### Table `mock_bunec_persons` (dev / test fixture)

Defined in
[`services/verification-engine/migrations/0001_initial.sql#L33-L38`](../../services/verification-engine/migrations/0001_initial.sql#L33-L38).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `person_id` | UUID PK | **PII** | [0001#L34](../../services/verification-engine/migrations/0001_initial.sql#L34) | This is a `person_id`; classified PII under § "Why some apparent identifiers are not PII". Even though this table holds synthetic dev/test data, the column name participates in OPS-2 redaction and the classification follows the data shape, not the data source. |
| `canonical_full_name` | TEXT NOT NULL | **PII** | [0001#L35](../../services/verification-engine/migrations/0001_initial.sql#L35) | Full legal name of a natural person. Production fixtures must remain synthetic; the real BUNEC integration is `R-VER-1`. |
| `nationality` | TEXT NOT NULL | **PII** | [0001#L36](../../services/verification-engine/migrations/0001_initial.sql#L36) | Two-letter country code; PII when paired with name. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L37](../../services/verification-engine/migrations/0001_initial.sql#L37) | Row insertion time; operator-surface. |

> **Operator note.** This table is out-of-scope for COMP-2 retention
> (`docs/compliance/data-retention.md` § Verification engine) because
> it carries no production data. The classifications above are
> non-negotiable nonetheless: the table is replaced by the real BUNEC
> adapter under R-VER-1, and the adapter's caching layer (when built)
> inherits the PII / Sensitive-PII shape documented in § Future tables.

### Table `verification_outbox`

Defined in
[`services/verification-engine/migrations/0001_initial.sql#L41-L53`](../../services/verification-engine/migrations/0001_initial.sql#L41-L53).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `id` | UUID PK DEFAULT gen_random_uuid() | Internal | [0001#L42](../../services/verification-engine/migrations/0001_initial.sql#L42) | |
| `event_id` | UUID NOT NULL UNIQUE | Public | [0001#L43](../../services/verification-engine/migrations/0001_initial.sql#L43) | |
| `event_type` | TEXT NOT NULL | Public | [0001#L44](../../services/verification-engine/migrations/0001_initial.sql#L44) | |
| `event_version` | INTEGER NOT NULL CHECK >=1 | Public | [0001#L45](../../services/verification-engine/migrations/0001_initial.sql#L45) | |
| `aggregate_id` | UUID NOT NULL | Public | [0001#L46](../../services/verification-engine/migrations/0001_initial.sql#L46) | |
| `partition_key` | TEXT NOT NULL | Internal | [0001#L47](../../services/verification-engine/migrations/0001_initial.sql#L47) | |
| `payload` | JSONB NOT NULL | **PII** | [0001#L48](../../services/verification-engine/migrations/0001_initial.sql#L48) | Writeback envelope; embeds case fields including `declarant_principal`. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0001#L49](../../services/verification-engine/migrations/0001_initial.sql#L49) | |
| `dispatched_at` | TIMESTAMPTZ NULL | Internal | [0001#L50](../../services/verification-engine/migrations/0001_initial.sql#L50) | |
| `dispatch_attempts` | INT NOT NULL DEFAULT 0 | Internal | [0001#L51](../../services/verification-engine/migrations/0001_initial.sql#L51) | |
| `last_error` | TEXT NULL | Internal | [0001#L52](../../services/verification-engine/migrations/0001_initial.sql#L52) | |

### Table `verification_outbox_dlq`

Defined in
[`services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L17-L30`](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L17-L30).

| Column | Type | Classification | Source | Notes |
|---|---|---|---|---|
| `id` | UUID PK | Internal | [0002#L18](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L18) | |
| `event_id` | UUID NOT NULL UNIQUE | Public | [0002#L19](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L19) | |
| `event_type` | TEXT NOT NULL | Public | [0002#L20](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L20) | |
| `event_version` | INTEGER NOT NULL | Public | [0002#L21](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L21) | |
| `aggregate_id` | UUID NOT NULL | Public | [0002#L22](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L22) | |
| `partition_key` | TEXT NOT NULL | Internal | [0002#L23](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L23) | |
| `payload` | JSONB NOT NULL | **PII** | [0002#L24](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L24) | |
| `created_at` | TIMESTAMPTZ NOT NULL | Internal | [0002#L26](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L26) | |
| `dead_lettered_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | [0002#L27](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L27) | |
| `dispatch_attempts` | INTEGER NOT NULL | Internal | [0002#L28](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L28) | |
| `last_error` | TEXT NULL | Internal | [0002#L29](../../services/verification-engine/migrations/0002_add_verification_outbox_dlq.sql#L29) | |

## Future tables

The two services below are planned in `docs/PRODUCTION-TODO.md` but not
yet built. The column lists below come directly from the tickets'
briefs and are pre-classified so the service owners inherit the
expected handling rules on Day 1. Any deviation requires an ADR.

### `[PLANNED]` `services/person-service` (ticket **R-DECL-4**)

A canonical natural-person registry that anchors every `person_id`
referenced inside `declarations.beneficial_owners`. Brief at
`docs/PRODUCTION-TODO.md` § R-DECL-4.

#### `[PLANNED]` Table `persons`

| Column | Planned type | Classification | Notes |
|---|---|---|---|
| `id` | UUID PK | Public | The `person_id` external handle. The UUID itself does not identify when isolated; pairing with `canonical_full_name` is what makes the table PII-laden. |
| `canonical_full_name` | TEXT NOT NULL | **PII** | Subject to GDPR / OHADA access + rectification + erasure rights as constrained by the AML/CFT carve-outs. Redaction at log boundaries enforced via OPS-2. |
| `nationality` | TEXT CHAR(2) NOT NULL | **PII** | ISO 3166-1 alpha-2 country code; PII in combination with the name. |
| `date_of_birth` | DATE NOT NULL | **PII** | Combined with name → high-confidence identity disambiguation; PII. |
| `primary_id_document` | JSONB NOT NULL | **Sensitive-PII** | Embeds {issuer, type, number, expiry}. Government-issued identity-document numbers are categorical Sensitive-PII. Field-level encryption REQUIRED at rest under placeholder ticket `R-ENC-FIELD-LEVEL`. Access audited per-row. |
| `biometric_reference_hash` | BYTEA NULL | **Sensitive-PII** | Hash of a biometric template (e.g. fingerprint). Biometric references are categorical Sensitive-PII per the most restrictive applicable regulation. Field-level encryption REQUIRED; per-row access audit; never exported to consumer APIs. |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | Operator-surface row creation timestamp. |

The Person service ships with its own append-only event log (analogous
to `declaration_events`) carrying `PersonRegistered`, `PersonUpdated`,
and `PersonMerged` events. The event log's `event_payload` column
inherits the **PII / Sensitive-PII** classification of whichever
column the event captures; encrypted-at-rest application is therefore
required on the event log row as well as on the projection.

### `[PLANNED]` `services/entity-service` (ticket **IDENTITY-1**)

Authoritative cache / projection of BUNEC business-register entries
(for Cameroonian entities) and declarant-submitted data verified
through the verification engine (for non-Cameroonian entities). Brief
at `docs/PRODUCTION-TODO.md` § IDENTITY-1.

#### `[PLANNED]` Table `entities`

| Column | Planned type | Classification | Notes |
|---|---|---|---|
| `id` | UUID PK | Public | The `entity_id` external handle. |
| `canonical_name` | TEXT NOT NULL | Public | Legal name of the entity; statutorily public under Cameroon's transparency framework. |
| `entity_type` | TEXT NOT NULL | Public | Enum {sa, sarl, partnership, …}. |
| `jurisdiction` | TEXT NOT NULL | Public | Jurisdiction of registration. |
| `registration_number_in_jurisdiction` | TEXT NOT NULL | Public | The business-register identifier; matches BUNEC's surface for Cameroonian entities. |
| `founded_at` | DATE NOT NULL | Public | Date of incorporation. |
| `dissolved_at` | DATE NULL | Public | Date of dissolution (if any). |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | Internal | Operator-surface row creation timestamp. |

The Entity service is **entirely Public + Internal** at the projection
layer: legal entities are not natural persons and therefore have no
PII columns of their own. Operator-internal columns (timestamps,
correlation IDs on the planned event log) are Internal; everything
else is Public.

## Cross-references

### How this document interacts with other compliance artefacts

- **`docs/compliance/gdpr-procedures.md` (COMP-1).** Every column
  classified PII or Sensitive-PII here is in scope for the six
  data-subject rights documented there: access (Art. 15), rectification
  (Art. 16), erasure (Art. 17 — constrained by OHADA AML/CFT
  carve-outs), restrict (Art. 18), portability (Art. 20), object
  (Art. 21). The procedures document gives the operational pathway;
  this document gives the per-column scope. PII fields are exposed
  through `GET /v1/declarations/by-principal`; Sensitive-PII fields
  (future) are exposed only via the audited per-row pathway.
- **`docs/compliance/data-retention.md` (COMP-2).** Retention per
  table; classification per column. The two interact: Public columns
  may be retained forever as part of the public-register record;
  PII / Sensitive-PII columns inside the same row inherit the retention
  rule of the row (the event log is retained forever; pruning a row
  to drop only its PII columns is impossible without breaking D15
  receipt-chain integrity). The platform's GDPR Art. 17 response for
  PII inside the event log is therefore the OHADA AML/CFT carve-out
  documented in `gdpr-procedures.md`, not row-level deletion.
- **`docs/compliance/regulatory-mapping.md` (COMP-4, not yet drafted).**
  Will cite which provisions of Cameroonian law and OHADA AML/CFT
  treat each Public column as a public-record obligation. Until COMP-4
  lands, the Public classification rests on counsel-pending citations
  in `gdpr-procedures.md`.

### How this document interacts with security artefacts

- **`docs/security/threat-model.md` (DOC-4) § Per-component STRIDE
  → Database.** That table's `I` row ("Postgres backup theft exposes
  PII") and its companion gap **G3** ("Declaration body PII unencrypted
  at rest in the projection table") cite this document for the
  per-column treatment. Field-level encryption on the Sensitive-PII
  columns described here will close part of G3 when
  `R-ENC-FIELD-LEVEL` ships; the projection's PII columns remain in
  scope for the broader encryption-at-rest ticket.
- **`packages/recor-logging/src/lib.rs` (OPS-2).** The redaction
  layer's `UUID_PII_FIELDS` constant
  ([L71-L76](../../packages/recor-logging/src/lib.rs#L71-L76))
  enumerates `person_id`, `principal`, `declarant_principal`, and
  `subject` as PII-class field names. The classification table above
  is consistent with that list: every column the table labels **PII**
  is reached through one of those field names at the tracing boundary
  (either directly, as for `declarant_principal`, or indirectly
  through the JSONB payload columns that embed `person_id` keys).
  Adding a new PII column whose field name is not in `UUID_PII_FIELDS`
  requires updating the constant in the same PR.

## Change procedure

1. New column or table: classify the field in the same PR that ships
   the migration. Add a row to the appropriate table above; cite the
   line range. Doctrine D05 (documentation is part of the feature).
2. Change of classification: requires an ADR documenting why the
   tier changes and what handling rule changes follow. Re-classifying
   a column from Internal / Confidential to PII implies the OPS-2
   layer must redact a new field name; re-classifying from PII to
   Sensitive-PII implies field-level encryption is now required.
3. Counsel sign-off: AML/CFT counsel reviews this document alongside
   `gdpr-procedures.md`. The PII / Sensitive-PII delineation is the
   load-bearing legal judgement and must be annotated by counsel
   before this document is treated as authoritative.

## References

- Architecture V1 P2 — doctrines (D14, D15, D17, D18)
- Architecture V4 P14 — canonical data model
- ADR-001 — event-sourcing on the declaration aggregate
- ADR-002 — Dempster-Shafer fusion math is deterministically replayable
- `docs/compliance/README.md` — index of compliance documents
- `docs/compliance/gdpr-procedures.md` — six data-subject rights
- `docs/compliance/data-retention.md` — retention per table
- `docs/security/threat-model.md` — STRIDE coverage + Gap G3
- `packages/recor-logging/src/lib.rs` — OPS-2 PII redaction layer
- `docs/PRODUCTION-TODO.md` § R-DECL-4 — Person service brief
- `docs/PRODUCTION-TODO.md` § IDENTITY-1 — Entity service brief
- `docs/PRODUCTION-TODO.md` § COMP-3 — this ticket's source brief
