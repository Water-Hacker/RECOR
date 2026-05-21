-- Migration: 0005_add_outbox_dlq
-- Service:   entity-service (TODO-039)
-- Sprint:    FATF final push
-- Rationale: mirrors services/declaration/migrations/0005_add_outbox_dlq.sql
--            for the entity service. The outbox relay worker (added in
--            this sprint at src/infrastructure/relay.rs) atomically
--            moves rows from `outbox` to `outbox_dlq` once
--            `dispatch_attempts` exhausts the configured max. The
--            forensic record (event_id, payload, last_error) is
--            preserved verbatim so on-call can correlate against the
--            downstream consumer's trace.
--
-- Atomicity: identical pattern to declaration —
--   BEGIN; INSERT INTO outbox_dlq ...; DELETE FROM outbox ...; COMMIT;
-- ensures a row exists in EXACTLY one of the two tables at any time.
--
-- Retention: outbox can be pruned 30 days after `dispatched_at` by the
-- existing OutboxRetention worker. outbox_dlq is NEVER pruned (forensic
-- record). Operators clear individual rows after manual review.
--
-- Properties verified post-migration:
--   1. outbox_dlq is empty initially.
--   2. The relay's UPDATE-only path remains unchanged when
--      dispatch_attempts < max_attempts; only the move-on-exhaust path
--      is new behaviour.

BEGIN;

CREATE TABLE IF NOT EXISTS outbox_dlq (
    id                  UUID PRIMARY KEY,
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    event_version       INTEGER NOT NULL,
    aggregate_type      TEXT NOT NULL,
    aggregate_id        UUID NOT NULL,
    partition_key       TEXT NOT NULL,
    payload             JSONB NOT NULL,
    headers             JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at          TIMESTAMPTZ NOT NULL,
    dead_lettered_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatch_attempts   INTEGER NOT NULL,
    last_error          TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_dlq_dead_lettered_at
    ON outbox_dlq (dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_outbox_dlq_aggregate
    ON outbox_dlq (aggregate_type, aggregate_id);

-- COMP-2: DLQ is forensic record. TRUNCATE forbidden; UPDATE/DELETE
-- left available for the manual replay/clear admin path.
REVOKE TRUNCATE ON outbox_dlq FROM PUBLIC;

COMMENT ON TABLE outbox_dlq IS
    'Dead-letter queue for outbox rows that exhausted dispatch_attempts. Forensic record; never auto-pruned. Replay via /v1/internal/outbox-dlq/{id}/replay (added with the relay worker).';

COMMIT;
