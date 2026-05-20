-- TODO-002 — FATF R.25 / INR.25 — Legal arrangements (trusts,
-- fiducies, waqf and similar) register.
--
-- ADR-0015 decided on a discriminated section of entity-service
-- rather than a new bounded context. The `arrangements` table sits
-- alongside `entities` and shares the principal-class /
-- authentication / outbox / COMP-2 substrate. The discriminator at
-- the API boundary is the URL prefix `/v1/arrangements` vs
-- `/v1/entities`; the discriminator at the data layer is the table
-- name. Person service is the source of natural-person identities
-- for the FKs.
--
-- The schema captures the six identifier roles R.25 names:
--   * settlor(s)
--   * trustee(s) (natural person, legal person, or registered fiduciary)
--   * protector(s)
--   * named beneficiaries
--   * class-described beneficiaries (free text + structured class
--     description for retrievability)
--   * other natural persons exercising ultimate effective control
--
-- The 5-year-after-cessation retention obligation (R.25 INR §3.f) is
-- enforced by a `retention_until` date — the retention worker
-- refuses to prune events before that date.

BEGIN;

CREATE TABLE IF NOT EXISTS arrangements (
    arrangement_id          UUID PRIMARY KEY,
    -- Discriminator: 'express_trust' | 'fiducy' | 'waqf' | 'similar'.
    -- The R.25-similar bucket is a catch-all for jurisdiction-specific
    -- equivalents (Liechtenstein Anstalt, Cayman STAR trust, etc.).
    arrangement_kind        TEXT NOT NULL CHECK (arrangement_kind IN (
        'express_trust', 'fiducy', 'waqf', 'similar'
    )),
    governing_law_jurisdiction TEXT NOT NULL,
    constitution_date       DATE NOT NULL,
    -- Optional dissolution / termination.
    dissolution_date        DATE,
    -- R.25 INR §3.f — 5-year-after-cessation retention.
    retention_until         DATE,
    -- Identifier-role columns: each is a JSONB array of `person_id` /
    -- `entity_id` references + the relationship metadata (when known
    -- and verified). The JSONB shape lets a single arrangement carry
    -- multiple settlors / trustees / etc. without spawning a join
    -- table per role.
    settlor_refs            JSONB NOT NULL DEFAULT '[]'::jsonb,
    trustee_refs            JSONB NOT NULL DEFAULT '[]'::jsonb,
    protector_refs          JSONB NOT NULL DEFAULT '[]'::jsonb,
    named_beneficiary_refs  JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- R.25 admits class-described beneficiaries ("my grandchildren").
    -- Captured as a structured class spec so a future investigator
    -- can resolve a named individual against the class definition.
    class_beneficiary_specs JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- "Other natural persons exercising ultimate effective control" —
    -- the catch-all R.25 names so a settlor-puppet-trustee scheme
    -- cannot evade disclosure.
    control_exercise_refs   JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- COMP-2-style provenance.
    created_by_principal    TEXT NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    aggregate_version       BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_arrangements_jurisdiction
    ON arrangements (governing_law_jurisdiction);
CREATE INDEX IF NOT EXISTS idx_arrangements_kind
    ON arrangements (arrangement_kind, constitution_date);
CREATE INDEX IF NOT EXISTS idx_arrangements_creator
    ON arrangements (created_by_principal);

CREATE TABLE IF NOT EXISTS arrangement_events (
    event_id        UUID PRIMARY KEY,
    arrangement_id  UUID NOT NULL REFERENCES arrangements (arrangement_id),
    event_type      TEXT NOT NULL,
    payload         JSONB NOT NULL,
    actor_principal TEXT NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL,
    sequence_no     BIGINT NOT NULL,
    UNIQUE (arrangement_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_arrangement_events_aggregate
    ON arrangement_events (arrangement_id, sequence_no);

-- COMP-2 immutability — the existing entity_events trigger function
-- is named `forbid_entity_events_mutation` (from migration 0001).
-- Verify by reading 0001 or its equivalent; if the function name
-- differs, fix this migration's references during apply.
CREATE OR REPLACE FUNCTION arrangement_events_refuse_mutation()
    RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'arrangement_events is COMP-2 append-only (FATF R.25 INR §3.f)';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS forbid_arrangement_events_update ON arrangement_events;
CREATE TRIGGER forbid_arrangement_events_update
    BEFORE UPDATE ON arrangement_events
    FOR EACH ROW EXECUTE FUNCTION arrangement_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_arrangement_events_delete ON arrangement_events;
CREATE TRIGGER forbid_arrangement_events_delete
    BEFORE DELETE ON arrangement_events
    FOR EACH ROW EXECUTE FUNCTION arrangement_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_arrangement_events_truncate ON arrangement_events;
CREATE TRIGGER forbid_arrangement_events_truncate
    BEFORE TRUNCATE ON arrangement_events
    FOR EACH STATEMENT EXECUTE FUNCTION arrangement_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON arrangement_events FROM PUBLIC;

COMMIT;
