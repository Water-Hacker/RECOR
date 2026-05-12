-- Migration: 0002_add_verification_outbox_dlq
-- Service:   verification-engine
-- Sprint:    PI-1 (R-LOOP-4-DLQ)
-- Rationale: mirror of services/declaration/migrations/0005 — gives
--            the verification engine's writeback relay a DLQ for
--            rows that exhaust dispatch_attempts. Same atomicity
--            properties (the relay does INSERT-then-DELETE inside
--            one transaction).
--
-- Note: verification_outbox doesn't have aggregate_type or headers
-- columns (smaller schema than the declaration outbox), so the DLQ
-- table is correspondingly smaller. If the schemas converge later,
-- the DLQs will too.

BEGIN;

CREATE TABLE IF NOT EXISTS verification_outbox_dlq (
    id                  UUID PRIMARY KEY,
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    event_version       INTEGER NOT NULL,
    aggregate_id        UUID NOT NULL,
    partition_key       TEXT NOT NULL,
    payload             JSONB NOT NULL,

    created_at          TIMESTAMPTZ NOT NULL,
    dead_lettered_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatch_attempts   INTEGER NOT NULL,
    last_error          TEXT
);

CREATE INDEX IF NOT EXISTS idx_verification_outbox_dlq_dead_lettered_at
    ON verification_outbox_dlq (dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_verification_outbox_dlq_aggregate
    ON verification_outbox_dlq (aggregate_id);

COMMIT;
