-- Migration: 0001_init
-- Service:   entity-service (IDENTITY-1)
-- Sprint:    Phase 1 (Entity service v1)
-- Author:    RÉCOR engineering
-- Forward-only: COMP-2 audit-immutability triggers below cannot be undone
-- by a downward migration; rolling back requires manual operator action
-- with a procedural audit trail. See services/declaration/migrations/0007
-- for the analogous rationale.
--
-- Rationale: initial schema for the Entity service. Legal entities are
-- the Public-classified counterparties in beneficial-ownership chains.
-- The service is the authoritative cache + projection of registry data:
-- for Cameroonian entities it eventually mirrors BUNEC (deferred to
-- R-VER-1), for non-Cameroonian entities it holds declarant-submitted
-- data verified through the verification engine.
--
-- Properties verified post-migration:
--   1. entities.id is PK
--   2. (entity_events.entity_id, aggregate_version) is UNIQUE
--   3. entity_events is UPDATE/DELETE/TRUNCATE-refused via trigger (COMP-2)
--   4. outbox.dispatched_at IS NULL identifies un-relayed events
--   5. idempotency_records.expires_at supports periodic cleanup
--   6. (jurisdiction, registration_number_in_jurisdiction) is UNIQUE
--      so two declarants cannot mint two different RÉCOR ids for the
--      same external-registry entry — a fail-closed invariant on
--      identity assignment (D14).
--
-- ╭─ Classification ───────────────────────────────────────────────╮
-- │ Per docs/compliance/data-classification.md § entities, every   │
-- │ column on the entities table is Public except `created_at` /   │
-- │ `updated_at`, which are Internal. The entity event log and     │
-- │ outbox carry no PII because legal entities are not natural     │
-- │ persons; the per-row classification is therefore Public +      │
-- │ Internal.                                                      │
-- ╰────────────────────────────────────────────────────────────────╯

BEGIN;

-- ── Current-state projection ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entities (
    id                                  UUID PRIMARY KEY,
    canonical_name                      TEXT NOT NULL CHECK (char_length(canonical_name) BETWEEN 1 AND 512),
    entity_type                         TEXT NOT NULL CHECK (char_length(entity_type) BETWEEN 1 AND 64),
    jurisdiction                        CHAR(2) NOT NULL CHECK (jurisdiction ~ '^[A-Z]{2}$'),
    registration_number_in_jurisdiction TEXT NOT NULL CHECK (char_length(registration_number_in_jurisdiction) BETWEEN 1 AND 128),
    founded_at                          DATE NOT NULL,
    dissolved_at                        DATE,
    aggregate_version                   BIGINT NOT NULL CHECK (aggregate_version >= 0),
    created_at                          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT entities_dissolution_after_foundation
        CHECK (dissolved_at IS NULL OR dissolved_at >= founded_at),
    CONSTRAINT entities_jurisdiction_registration_unique
        UNIQUE (jurisdiction, registration_number_in_jurisdiction)
);

CREATE INDEX IF NOT EXISTS idx_entities_jurisdiction
    ON entities (jurisdiction, founded_at DESC);

CREATE INDEX IF NOT EXISTS idx_entities_entity_type
    ON entities (entity_type);

-- Substring/ILIKE search on canonical_name. trigram index keeps the
-- search endpoint sub-millisecond for the v1 directory cardinality
-- (~hundreds of thousands of rows); the planner falls back to a seq
-- scan if pg_trgm is unavailable, which is acceptable for v1 traffic.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS idx_entities_canonical_name_trgm
    ON entities USING gin (canonical_name gin_trgm_ops);

-- Auto-update the projection's updated_at timestamp on every UPDATE.
CREATE OR REPLACE FUNCTION entities_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_entities_updated_at ON entities;
CREATE TRIGGER trg_entities_updated_at
    BEFORE UPDATE ON entities
    FOR EACH ROW
    EXECUTE FUNCTION entities_set_updated_at();

-- ── Append-only event log ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entity_events (
    seq_id              BIGSERIAL PRIMARY KEY,
    entity_id           UUID NOT NULL,
    aggregate_version   BIGINT NOT NULL CHECK (aggregate_version >= 1),
    event_type          TEXT NOT NULL,
    event_payload       JSONB NOT NULL,
    event_time          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    correlation_id      UUID NOT NULL,
    causation_id        UUID,
    UNIQUE (entity_id, aggregate_version)
);

CREATE INDEX IF NOT EXISTS idx_entity_events_aggregate
    ON entity_events (entity_id, aggregate_version);

CREATE INDEX IF NOT EXISTS idx_entity_events_time
    ON entity_events (event_time DESC);

-- COMP-2: append-only immutability. UPDATE/DELETE/TRUNCATE refused by
-- trigger. The application never issues these; the trigger documents
-- and enforces the contract at the SQL boundary so a future bug, a
-- compromised connection, or a misguided DBA cannot silently rewrite
-- history. Identical pattern to declaration_events (migration 0007).
CREATE OR REPLACE FUNCTION entity_events_refuse_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'entity_events is append-only (COMP-2); % refused by trigger',
        TG_OP
    USING ERRCODE = 'insufficient_privilege';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_entity_events_no_update ON entity_events;
CREATE TRIGGER trg_entity_events_no_update
    BEFORE UPDATE ON entity_events
    FOR EACH ROW EXECUTE FUNCTION entity_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_entity_events_no_delete ON entity_events;
CREATE TRIGGER trg_entity_events_no_delete
    BEFORE DELETE ON entity_events
    FOR EACH ROW EXECUTE FUNCTION entity_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_entity_events_no_truncate ON entity_events;
CREATE TRIGGER trg_entity_events_no_truncate
    BEFORE TRUNCATE ON entity_events
    FOR EACH STATEMENT EXECUTE FUNCTION entity_events_refuse_mutation();

-- Belt-and-braces: strip mutating privileges from PUBLIC so any
-- non-owner role inherits deny-by-default.
REVOKE UPDATE, DELETE, TRUNCATE ON entity_events FROM PUBLIC;

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
    ON idempotency_records (expires_at);

-- ── Outbox (Doctrine 5/Principle 5: Outbox pattern) ────────────────────────
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

-- COMP-2: outbox TRUNCATE is forbidden. UPDATE/DELETE are retained
-- (relay marks dispatched, retention worker prunes after dispatch).
REVOKE TRUNCATE ON outbox FROM PUBLIC;

-- ── Commentary ─────────────────────────────────────────────────────────────

COMMENT ON TABLE entities IS
    'Current-state projection of the legal-entity registry. Public + Internal classification only (no PII). See docs/compliance/data-classification.md § entities.';
COMMENT ON TABLE entity_events IS
    'Append-only entity event log. UPDATE/DELETE/TRUNCATE refused by trigger (COMP-2). Retained forever (D15 cryptographic provenance).';
COMMENT ON TABLE outbox IS
    'Outbox for at-least-once event delivery. INSERT/SELECT/UPDATE/DELETE permitted; TRUNCATE forbidden. Dispatched rows are pruned by the retention worker (COMP-2).';
COMMENT ON CONSTRAINT entities_jurisdiction_registration_unique ON entities IS
    'Identity invariant: a single (jurisdiction, registration_number_in_jurisdiction) pair resolves to exactly one RÉCOR entity_id. Prevents two declarants from minting two different ids for the same external-registry entry.';

COMMIT;
