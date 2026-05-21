# Data dictionary — planned tables

**Last updated:** 2026-05-20

These tables are planned in `docs/PRODUCTION-TODO.md` but not yet built.
The column lists come from the ticket briefs and ADRs; they are
pre-classified so the service owners inherit the correct handling rules
on Day 1. Any deviation from these classifications requires an ADR.

---

## `[PLANNED]` Table `persons` — `services/person-service` (R-DECL-4)

A canonical natural-person registry that anchors every `person_id`
referenced inside `declarations.beneficial_owners`. Brief at
`docs/PRODUCTION-TODO.md` § R-DECL-4. Pre-classified in
`docs/compliance/data-classification.md` § Future tables.

| Column | Planned type | Constraints | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|---|
| `id` | `UUID PK` | PK | Public | Art. 6(1)(c) — person_id is the external handle; pairing with `canonical_full_name` creates PII | Forever |
| `canonical_full_name` | `TEXT NOT NULL` | — | **PII** | Art. 6(1)(c) — FATF cascade requires the full legal name | Forever — OHADA AML/CFT carve-out; erasure constrained by statutory retention |
| `nationality` | `CHAR(2) NOT NULL` | ISO 3166-1 alpha-2 | **PII** | Art. 6(1)(c) — PII in combination with the name | Forever |
| `date_of_birth` | `DATE NOT NULL` | — | **PII** | Art. 6(1)(c) — identity disambiguation; PII combined with name | Forever |
| `primary_id_document` | `JSONB NOT NULL` | — | **Sensitive-PII** | Art. 6(1)(c); field-level encryption REQUIRED (`R-ENC-FIELD-LEVEL` prerequisite to activation); per-row access audit required | Forever — AML/CFT carve-out |
| `biometric_reference_hash` | `BYTEA NULL` | — | **Sensitive-PII** | Art. 6(1)(c) — biometric template hash; field-level encryption REQUIRED; per-row access audit; never exported to consumer APIs | Forever — AML/CFT carve-out |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |

**Prerequisite.** `primary_id_document` and `biometric_reference_hash`
must not be activated until `R-ENC-FIELD-LEVEL` ships, the field-level
encryption is validated, and this DPIA is amended (see
`docs/compliance/dpia.md` § 5.3, R-BO-06).

---

## `[PLANNED]` Table `person_events` — `services/person-service`

The append-only event log for the person aggregate, analogous to
`declaration_events`. All columns carrying PII/Sensitive-PII in the
`persons` projection inherit the same classification in the event payload.

| Column | Planned type | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|
| `event_id` | `UUID PK` | Public | Art. 6(1)(c) | **Forever** — immutable |
| `person_id` | `UUID NOT NULL` | **PII** | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `event_payload` | `JSONB NOT NULL` | **PII / Sensitive-PII** (inherits from `persons`) | Art. 6(1)(c) + Art. 17(3)(b) exemption | **Forever** — immutable; OHADA carve-out |
| `occurred_at` | `TIMESTAMPTZ NOT NULL` | Internal | Art. 6(1)(c) | Forever |
| `actor_principal` | `TEXT NOT NULL` | **PII** | Art. 6(1)(c) | Forever |
| `aggregate_version` | `BIGINT NOT NULL` | Confidential | Art. 6(1)(c) | Forever |

---

## `[PLANNED]` Table `entities` — `services/entity-service` (IDENTITY-1)

Authoritative cache of BUNEC business-register entries for Cameroonian
entities and declarant-submitted data for non-Cameroonian entities.
**Entirely Public + Internal**: legal entities are not natural persons and
carry no PII columns of their own.

| Column | Planned type | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|
| `id` | `UUID PK` | Public | Art. 6(1)(c) | Forever |
| `canonical_name` | `TEXT NOT NULL` | Public | Art. 6(1)(c) — statutory public record | Forever |
| `entity_type` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `jurisdiction` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `registration_number_in_jurisdiction` | `TEXT NOT NULL` | Public | Art. 6(1)(c) — business-register ID | Forever |
| `founded_at` | `DATE NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `dissolved_at` | `DATE NULL` | Public | Art. 6(1)(c) | Forever |
| `has_outstanding_bearer_shares` | `BOOLEAN NOT NULL` | DEFAULT false | Public | Art. 6(1)(c) — FATF c.24.12 bearer-share disclosure | Forever |
| `bearer_share_status` | `TEXT NOT NULL` | CHECK IN ('none','outstanding','converted','immobilised') | Public | Art. 6(1)(c) | Forever |
| `sufficient_link_kind` | `TEXT NULL` | CHECK IN ('branch','significant_business','financial_relationship','real_estate','employees','tax_residence','other_documented') | Public | Art. 6(1)(c) — FATF c.24.1(d) sufficient-link test | Forever |
| `sufficient_link_evidence` | `TEXT NULL` | 16–2048 chars | Public | Art. 6(1)(c) | Forever |
| `created_at` | `TIMESTAMPTZ NOT NULL` | DEFAULT NOW() | Internal | Art. 6(1)(c) | Forever |

---

## `[PLANNED]` Table `entity_events` — `services/entity-service`

| Column | Planned type | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|
| `event_id` | `UUID PK` | Public | Art. 6(1)(c) | **Forever** — immutable |
| `entity_id` | `UUID NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `event_type` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `event_payload` | `JSONB NOT NULL` | Public | Art. 6(1)(c) — entity data is statutory public record | Forever |
| `occurred_at` | `TIMESTAMPTZ NOT NULL` | Internal | Art. 6(1)(c) | Forever |

---

## `[PLANNED]` Table `arrangements` — `services/declaration` (R-DECL-5)

Captures trust and similar arrangements required by FATF R.25. The
`arrangements` table extends the declaration domain to cover legal
arrangements (trusts, foundations, fideicommissa) in addition to legal
persons. Detailed schema is pending ticket R-DECL-5; the pre-classification
below establishes the expected handling rules.

| Column | Planned type | Classification | GDPR legal basis | Retention |
|---|---|---|---|---|
| `arrangement_id` | `UUID PK` | Public | Art. 6(1)(c) | Forever |
| `arrangement_type` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `jurisdiction` | `TEXT NOT NULL` | Public | Art. 6(1)(c) | Forever |
| `trustee_person_ids` | `UUID[] NOT NULL` | **PII** — person identifiers | Art. 6(1)(c) | Forever |
| `settlor_person_ids` | `UUID[] NOT NULL` | **PII** | Art. 6(1)(c) | Forever |
| `beneficiary_person_ids` | `UUID[] NOT NULL` | **PII** | Art. 6(1)(c) — FATF R.25 requires beneficiary disclosure | Forever |
| `protector_person_ids` | `UUID[] NULL` | **PII** | Art. 6(1)(c) | Forever |
| `declaration_id` | `UUID NOT NULL` | FK | Public | Art. 6(1)(c) | Forever |
| `submitted_at` | `TIMESTAMPTZ NOT NULL` | — | Internal | Art. 6(1)(c) | Forever |

---

## References

- `docs/PRODUCTION-TODO.md` — ticket briefs for planned services
- `docs/compliance/data-classification.md` (COMP-3) § Future tables
- `docs/compliance/dpia.md` (COMP-6) § 2.3 — special categories for Sensitive-PII
- `docs/adr/0010-fatf-bo-cascade-and-adequacy.md` — bearer-share + sufficient-link fields
- Architecture V4 P14 § Canonical data model
