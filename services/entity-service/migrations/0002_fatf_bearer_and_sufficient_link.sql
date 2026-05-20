-- Migration: 0002_fatf_bearer_and_sufficient_link
-- Service:   entity-service
-- Sprint:    PR-FATF-2.A (FATF-readiness Pass 2 — domain layer)
-- Author:    RÉCOR engineering
-- Forward-only: schema columns + CHECK constraints; rollback by adding
-- a downward migration that drops the columns (rows added since this
-- migration will lose those values — that is the design intent of a
-- forward-only schema and the rollback documentation).
--
-- Rationale: closes TODO-010 (bearer-share disclosure per FATF R.24
-- c.24.12) and TODO-018 (foreign-entity "sufficient link" test per
-- c.24.1(d) fn 15). Both are unwaivable in any external mutual
-- evaluation review — Cameroon's MER will assess R.24 against the
-- presence of these fields.
--
-- Properties verified post-migration:
--   1. entities.has_outstanding_bearer_shares is NOT NULL with default false.
--   2. entities.bearer_share_status carries a constrained enum value.
--   3. Foreign entities (jurisdiction != 'CM') with NULL sufficient_link_kind
--      are flagged by a CHECK constraint that runs on every INSERT/UPDATE.
--   4. entities.sufficient_link_evidence may be NULL for domestic entities
--      and carries up to 2048 chars for foreign entities.
--   5. The entity_events log is unaltered; new fields appear inside the
--      event payload JSON. Historical events deserialise via the Rust
--      `#[serde(default)]` machinery.

BEGIN;

-- ── Bearer-share disclosure (TODO-010 / FATF R.24 c.24.12) ──────────
--
-- c.24.12(a) — countries MUST prohibit issuance of new bearer shares.
-- c.24.12(b) — existing bearer shares MUST be converted to registered
--              form or immobilised at a regulated intermediary.
--
-- The platform's role: capture the declarant's structured attestation
-- of bearer-share status on every entity registration + update event.
-- Whether to enforce "no new bearer shares" is the back-office +
-- regulatory layer's job; the registry's job is to *know*.

ALTER TABLE entities
    ADD COLUMN IF NOT EXISTS has_outstanding_bearer_shares BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE entities
    ADD COLUMN IF NOT EXISTS bearer_share_status TEXT NOT NULL DEFAULT 'none'
        CHECK (bearer_share_status IN ('none', 'outstanding', 'converted', 'immobilised'));

-- Cross-field constraint: if `has_outstanding_bearer_shares` is true,
-- the status MUST be `outstanding`; if false, the status MUST be `none`,
-- `converted`, or `immobilised`. This prevents the schema from carrying
-- contradictory rows.
ALTER TABLE entities
    ADD CONSTRAINT entities_bearer_status_consistent CHECK (
        (has_outstanding_bearer_shares = true  AND bearer_share_status = 'outstanding') OR
        (has_outstanding_bearer_shares = false AND bearer_share_status IN ('none', 'converted', 'immobilised'))
    );

COMMENT ON COLUMN entities.has_outstanding_bearer_shares IS
    'TODO-010 closure — FATF R.24 c.24.12 disclosure. True means at '
    'least one bearer share or bearer warrant is outstanding at the '
    'attestation moment; the bearer_share_status column then carries '
    '"outstanding" with the remediation track recorded out-of-band.';

COMMENT ON COLUMN entities.bearer_share_status IS
    'TODO-010 closure — FATF R.24 c.24.12 remediation tracking. '
    '"none" = the entity has never issued bearer instruments; '
    '"outstanding" = at least one is outstanding (cross-checked '
    'against has_outstanding_bearer_shares); "converted" = formerly '
    'outstanding, now registered; "immobilised" = held at a regulated '
    'intermediary per c.24.12(b)(ii).';


-- ── Sufficient-link test for foreign entities (TODO-018) ────────────
--
-- c.24.1(d) fn 15 lists the canonical sufficient-link patterns: branch,
-- significant business, FI/DNFBP relationship, real estate, employees,
-- tax residence. The registry asserts which (any-of) applies; the
-- back-office reviewer validates the supporting evidence.
--
-- For domestic entities (jurisdiction = 'CM' under ISO 3166), this
-- field MAY be NULL — they have no sufficient-link test to satisfy.
-- For foreign entities (jurisdiction != 'CM'), the field MUST be set
-- (enforced by the CHECK below). The aggregate's `handle_register` use
-- case refuses foreign-entity registrations that don't carry it.

ALTER TABLE entities
    ADD COLUMN IF NOT EXISTS sufficient_link_kind TEXT NULL
        CHECK (sufficient_link_kind IS NULL OR sufficient_link_kind IN (
            'branch',
            'significant_business',
            'financial_relationship',
            'real_estate',
            'employees',
            'tax_residence',
            'other_documented'
        ));

ALTER TABLE entities
    ADD COLUMN IF NOT EXISTS sufficient_link_evidence TEXT NULL
        CHECK (sufficient_link_evidence IS NULL OR char_length(sufficient_link_evidence) BETWEEN 16 AND 2048);

-- The foreign-entity rule: jurisdiction != 'CM' implies sufficient_link_kind
-- must be set. We use NOT VALID + a one-time validation to apply the rule
-- only on newly-inserted / updated rows; existing rows aren't punished by
-- the migration. The constraint becomes effective for new writes.
--
-- (Historical projection rows had no sufficient_link concept; back-fill
-- is the operator's job under the R-VER-1 / BUNEC integration ticket,
-- where authoritative provenance becomes available.)
ALTER TABLE entities
    ADD CONSTRAINT entities_foreign_requires_sufficient_link CHECK (
        jurisdiction = 'CM' OR sufficient_link_kind IS NOT NULL
    ) NOT VALID;

COMMENT ON COLUMN entities.sufficient_link_kind IS
    'TODO-018 closure — FATF R.24 c.24.1(d) fn 15. Required for '
    'jurisdiction != ''CM''. Identifies which sufficient-link pattern '
    'applies; supporting evidence carried in sufficient_link_evidence.';

COMMENT ON COLUMN entities.sufficient_link_evidence IS
    'TODO-018 closure — free-text evidence (16–2048 chars) supporting '
    'the sufficient_link_kind. Back-office reviewer validates semantics.';

COMMIT;
