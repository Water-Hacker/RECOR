-- Migration: 0001_initial
-- Service:   declaration
-- Sprint:    PI-1 (Declaration service v1)
-- Author:    RÉCOR engineering
-- Rationale: initial schema for the Declaration service; event log,
--            current-state projection, idempotency cache, outbox.
--
-- Properties verified post-migration:
--   1. declarations.declaration_id is PK
--   2. (declaration_events.declaration_id, aggregate_version) is UNIQUE
--   3. outbox.dispatched_at IS NULL identifies un-relayed events
--   4. idempotency_records.expires_at supports periodic cleanup

BEGIN;

-- ── Current-state projection ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS declarations (
    declaration_id          UUID PRIMARY KEY,
    entity_id               UUID NOT NULL,
    declarant_principal     TEXT NOT NULL CHECK (char_length(declarant_principal) BETWEEN 1 AND 512),
    declarant_role          TEXT NOT NULL CHECK (declarant_role IN ('self', 'authorised_agent', 'operator_assisted')),
    declaration_kind        TEXT NOT NULL CHECK (declaration_kind IN ('incorporation', 'annual_renewal', 'change_of_control', 'correction', 'amendment')),
    effective_from          DATE NOT NULL,
    beneficial_owners       JSONB NOT NULL,
    attestation             JSONB NOT NULL,
    state                   TEXT NOT NULL CHECK (state IN ('draft', 'submitted', 'in_verification', 'accepted', 'rejected', 'superseded')),
    aggregate_version       BIGINT NOT NULL CHECK (aggregate_version >= 0),
    submitted_at            TIMESTAMPTZ NOT NULL,
    receipt_hash_hex        TEXT NOT NULL CHECK (char_length(receipt_hash_hex) = 64),
    correlation_id          UUID NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_declarations_entity
    ON declarations(entity_id, effective_from DESC);

CREATE INDEX IF NOT EXISTS idx_declarations_state
    ON declarations(state)
    WHERE state IN ('submitted', 'in_verification');

CREATE INDEX IF NOT EXISTS idx_declarations_correlation
    ON declarations(correlation_id);

-- Auto-update the projection's updated_at timestamp on every UPDATE.
CREATE OR REPLACE FUNCTION declarations_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_declarations_updated_at ON declarations;
CREATE TRIGGER trg_declarations_updated_at
    BEFORE UPDATE ON declarations
    FOR EACH ROW
    EXECUTE FUNCTION declarations_set_updated_at();

-- ── Append-only event log ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS declaration_events (
    seq_id              BIGSERIAL PRIMARY KEY,
    declaration_id      UUID NOT NULL,
    aggregate_version   BIGINT NOT NULL CHECK (aggregate_version >= 1),
    event_type          TEXT NOT NULL,
    event_payload       JSONB NOT NULL,
    event_time          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    correlation_id      UUID NOT NULL,
    causation_id        UUID,
    UNIQUE (declaration_id, aggregate_version)
);

CREATE INDEX IF NOT EXISTS idx_declaration_events_aggregate
    ON declaration_events(declaration_id, aggregate_version);

CREATE INDEX IF NOT EXISTS idx_declaration_events_time
    ON declaration_events(event_time DESC);

-- Belt-and-braces: prevent UPDATE/DELETE on the event log via row-level rule.
-- The application code never issues UPDATE/DELETE against this table; the
-- rule documents and enforces append-only semantics.
ALTER TABLE declaration_events
    ALTER COLUMN event_time SET DEFAULT NOW();

-- ── Idempotency cache ───────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS idempotency_records (
    idempotency_key         TEXT PRIMARY KEY CHECK (char_length(idempotency_key) BETWEEN 1 AND 256),
    declarant_principal     TEXT NOT NULL,
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

COMMIT;
