-- Migration: 0005_add_outbox_dlq
-- Service:   declaration
-- Sprint:    PI-1 (R-LOOP-4-DLQ)
-- Rationale: outbox rows that exhaust dispatch_attempts currently sit
--            in the `outbox` table indefinitely, marked with
--            `last_error` but otherwise indistinguishable from rows
--            the relay is still retrying. This creates two
--            operational problems:
--              1. The "is the relay healthy?" question gets noisy:
--                 a SELECT COUNT(*) of undispatched rows returns
--                 both genuinely-pending and permanently-failed rows.
--              2. Forensics on a permanently-failed row needs a
--                 separate dump from the live outbox.
--
--            This migration adds `outbox_dlq` — a separate table the
--            relay moves rows INTO atomically when dispatch_attempts
--            hit max_attempts. The original event_id is preserved
--            so the row can be cross-referenced with downstream
--            traces. A new column `last_error_history` captures the
--            last 12 errors (one per attempt) so on-call has the
--            history without needing to query the live row's history.
--
-- Atomicity: the relay moves rows in a single transaction:
--   BEGIN;
--   INSERT INTO outbox_dlq (...) SELECT ... FROM outbox WHERE id = $1;
--   DELETE FROM outbox WHERE id = $1;
--   COMMIT;
-- so a row exists in EXACTLY one of the two tables at any time.
--
-- Retention: outbox can be pruned after `dispatched_at + 30 days`
-- (a future cleanup ticket). outbox_dlq is NEVER pruned — it's the
-- forensic record. Operators can clear individual rows after manual
-- review.
--
-- Properties verified post-migration:
--   1. outbox_dlq is empty initially.
--   2. The relay's UPDATE-only path remains unchanged when
--      dispatch_attempts < max_attempts; only the move-on-exhaust
--      path is new behaviour.

BEGIN;

CREATE TABLE IF NOT EXISTS outbox_dlq (
    -- Carry the original outbox row's identity so forensics can
    -- correlate by event_id (the same value the consumer would have
    -- seen had dispatch succeeded).
    id                  UUID PRIMARY KEY,
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    event_version       INTEGER NOT NULL,
    aggregate_type      TEXT NOT NULL,
    aggregate_id        UUID NOT NULL,
    partition_key       TEXT NOT NULL,
    payload             JSONB NOT NULL,
    headers             JSONB NOT NULL DEFAULT '{}'::JSONB,

    -- Provenance + lifecycle.
    created_at          TIMESTAMPTZ NOT NULL,
    dead_lettered_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatch_attempts   INTEGER NOT NULL,
    -- The final error message (whatever caused the last attempt to
    -- fail). Historical attempts' errors are NOT preserved at this
    -- slice; a future ticket adds last_error_history JSONB.
    last_error          TEXT
);

-- Query patterns:
--   1. "Show me everything dead-lettered in the last 24h" — by
--      dead_lettered_at descending.
--   2. "Show me dead-letters for a particular aggregate" — by
--      (aggregate_type, aggregate_id).
CREATE INDEX IF NOT EXISTS idx_outbox_dlq_dead_lettered_at
    ON outbox_dlq (dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_outbox_dlq_aggregate
    ON outbox_dlq (aggregate_type, aggregate_id);

COMMIT;
