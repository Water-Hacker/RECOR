-- Migration: 0001_init
-- Service:   person-service (R-DECL-4)
-- Sprint:    PI-1
-- Author:    RÉCOR engineering
-- Rationale: initial schema for the Person service; canonical natural-
--            person registry, event log, idempotency cache, outbox.
--
-- Properties verified post-migration:
--   1. persons.person_id is PK
--   2. (person_events.person_id, aggregate_version) is UNIQUE
--   3. person_events is immutable via BEFORE UPDATE/DELETE/TRUNCATE triggers
--      (mirror of services/declaration migration 0007_audit_log_immutability)
--   4. outbox.dispatched_at IS NULL identifies un-relayed rows

BEGIN;

-- ── Current-state projection ────────────────────────────────────────────────
--
-- PII / Sensitive-PII classification per
-- docs/compliance/data-classification.md § "[PLANNED] services/person-service":
--   * canonical_full_name, nationality, date_of_birth    → PII
--   * primary_id_document, biometric_reference_hash      → Sensitive-PII
--     (field-level encryption REQUIRED once R-ENC-FIELD-LEVEL ships)
CREATE TABLE IF NOT EXISTS persons (
    person_id                 UUID PRIMARY KEY,
    canonical_full_name       TEXT NOT NULL CHECK (char_length(canonical_full_name) BETWEEN 1 AND 512),
    -- ISO 3166-1 alpha-2 (two ASCII uppercase letters). The 2-char
    -- check at the DB layer mirrors the value-object constructor.
    nationality               CHAR(2) NOT NULL CHECK (nationality ~ '^[A-Z]{2}$'),
    -- Nullable for legacy beneficial-owner records imported without a
    -- verified DOB.
    date_of_birth             DATE NULL,
    -- {issuer, doc_type, number, expiry}. Sensitive-PII; treated as a
    -- single sealed blob for v1.
    primary_id_document       JSONB NOT NULL,
    -- Hash of a biometric template; never the template itself.
    -- Sensitive-PII per the classification matrix.
    biometric_reference_hash  TEXT NULL CHECK (
        biometric_reference_hash IS NULL
        OR (char_length(biometric_reference_hash) BETWEEN 64 AND 128
            AND biometric_reference_hash ~ '^[0-9a-fA-F]+$')
    ),
    -- Optional pointer to the surviving canonical record after a merge.
    -- NULL for live records; non-NULL means the row is a merged-out shell.
    merged_into               UUID NULL REFERENCES persons(person_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    aggregate_version         BIGINT NOT NULL CHECK (aggregate_version >= 0),
    created_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Sanity: a person can't be merged into themselves.
    CONSTRAINT persons_no_self_merge CHECK (merged_into IS NULL OR merged_into <> person_id)
);

COMMENT ON TABLE persons IS
    'Current-state projection of the Person aggregate. PII + Sensitive-PII per docs/compliance/data-classification.md § Person service. Field-level encryption on the Sensitive-PII columns is the R-ENC-FIELD-LEVEL follow-up.';
COMMENT ON COLUMN persons.canonical_full_name IS 'PII; subject to GDPR Art. 15/16/17/20 as constrained by OHADA AML/CFT carve-outs.';
COMMENT ON COLUMN persons.nationality IS 'ISO 3166-1 alpha-2. PII in combination with canonical_full_name.';
COMMENT ON COLUMN persons.date_of_birth IS 'PII. Nullable for legacy records imported without a verified DOB.';
COMMENT ON COLUMN persons.primary_id_document IS 'Sensitive-PII. Government-issued identity-document body {issuer, doc_type, number, expiry}.';
COMMENT ON COLUMN persons.biometric_reference_hash IS 'Sensitive-PII. Hash of a biometric template, NEVER the template itself.';

CREATE INDEX IF NOT EXISTS idx_persons_canonical_full_name
    ON persons USING btree (canonical_full_name)
    WHERE merged_into IS NULL;

CREATE INDEX IF NOT EXISTS idx_persons_nationality
    ON persons (nationality)
    WHERE merged_into IS NULL;

-- Auto-update the projection's updated_at on every UPDATE.
CREATE OR REPLACE FUNCTION persons_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_persons_updated_at ON persons;
CREATE TRIGGER trg_persons_updated_at
    BEFORE UPDATE ON persons
    FOR EACH ROW
    EXECUTE FUNCTION persons_set_updated_at();

-- ── Append-only event log ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS person_events (
    seq_id              BIGSERIAL PRIMARY KEY,
    person_id           UUID NOT NULL,
    aggregate_version   BIGINT NOT NULL CHECK (aggregate_version >= 1),
    event_type          TEXT NOT NULL CHECK (event_type IN (
                            'person.registered.v1',
                            'person.updated.v1',
                            'person.merged.v1'
                        )),
    event_payload       JSONB NOT NULL,
    event_time          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    correlation_id      UUID NOT NULL,
    causation_id        UUID,
    UNIQUE (person_id, aggregate_version)
);

CREATE INDEX IF NOT EXISTS idx_person_events_aggregate
    ON person_events(person_id, aggregate_version);

CREATE INDEX IF NOT EXISTS idx_person_events_time
    ON person_events(event_time DESC);

-- ── COMP-2 mirror: enforce append-only at the trigger boundary ──────────────
--
-- Mirrors services/declaration migration 0007_audit_log_immutability for
-- the person_events log. Doctrine 15 (cryptographic provenance) requires
-- every consequential event to be tamper-evident; the application code
-- in `src/infrastructure/postgres.rs` only ever INSERTs against this
-- table. The triggers enforce that contract even when the invoking role
-- happens to be the table owner (REVOKE alone is a no-op against the
-- owner). The integration test in
-- `services/person-service/tests/audit_immutability.rs` asserts the
-- refusal path.
CREATE OR REPLACE FUNCTION person_events_refuse_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'person_events is append-only (COMP-2); % refused by trigger',
        TG_OP
    USING ERRCODE = 'insufficient_privilege';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_person_events_no_update ON person_events;
CREATE TRIGGER trg_person_events_no_update
    BEFORE UPDATE ON person_events
    FOR EACH ROW EXECUTE FUNCTION person_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_person_events_no_delete ON person_events;
CREATE TRIGGER trg_person_events_no_delete
    BEFORE DELETE ON person_events
    FOR EACH ROW EXECUTE FUNCTION person_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_person_events_no_truncate ON person_events;
CREATE TRIGGER trg_person_events_no_truncate
    BEFORE TRUNCATE ON person_events
    FOR EACH STATEMENT EXECUTE FUNCTION person_events_refuse_mutation();

-- Belt-and-braces: strip UPDATE/DELETE/TRUNCATE from PUBLIC. In the
-- separate-app-role deployment this is the load-bearing guarantee
-- (the owner-bypassing trigger above is the catch-all).
REVOKE UPDATE, DELETE, TRUNCATE ON person_events FROM PUBLIC;

COMMENT ON TABLE person_events IS
    'Append-only event log for the Person aggregate. UPDATE/DELETE/TRUNCATE refused by trigger (COMP-2). Retained forever (D15 cryptographic provenance).';

-- ── Idempotency cache ───────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS idempotency_records (
    idempotency_key         TEXT PRIMARY KEY CHECK (char_length(idempotency_key) BETWEEN 1 AND 256),
    actor_principal         TEXT NOT NULL,
    request_hash            TEXT NOT NULL CHECK (char_length(request_hash) = 64),
    response_status         SMALLINT NOT NULL CHECK (response_status BETWEEN 100 AND 599),
    response_body           JSONB NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at              TIMESTAMPTZ NOT NULL,
    CONSTRAINT idempotency_expires_after_creation
        CHECK (expires_at > created_at)
);

CREATE INDEX IF NOT EXISTS idx_idempotency_expires
    ON idempotency_records(expires_at);

-- ── Outbox (mirror of declaration's outbox shape) ──────────────────────────
CREATE TABLE IF NOT EXISTS outbox (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    event_version       INTEGER NOT NULL CHECK (event_version >= 1),
    aggregate_type      TEXT NOT NULL,
    aggregate_id        UUID NOT NULL,
    partition_key       TEXT NOT NULL,
    payload             JSONB NOT NULL,
    headers             JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatched_at       TIMESTAMPTZ,
    dispatch_attempts   INT NOT NULL DEFAULT 0 CHECK (dispatch_attempts >= 0),
    last_error          TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_undispatched
    ON outbox (created_at)
    WHERE dispatched_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_outbox_aggregate
    ON outbox (aggregate_type, aggregate_id, created_at);

-- TRUNCATE on the outbox is forbidden — truncating would silently drop
-- un-relayed events.
REVOKE TRUNCATE ON outbox FROM PUBLIC;

COMMENT ON TABLE outbox IS
    'Outbox for at-least-once event delivery. Mirrors the declaration service shape so the future outbox-relay worker is reusable across services.';

COMMIT;
