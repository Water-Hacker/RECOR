-- Migration: 0010_last_event_observed_at
-- Service:   declaration
-- Sprint:    PR-FATF-4 (FATF-readiness Pass 4 — 30-day update obligation)
-- Author:    RÉCOR engineering
-- Rationale: closes the storage half of TODO-005 from the production-
--            readiness audit (TODOS.md) — FATF R.24 §c.24.8 fn 29
--            requires BO data to be "updated within a reasonable
--            period following any change; FATF benchmark: within one
--            month". The platform needs to know WHEN a BO change
--            actually happened (the declarant asserts this) so a
--            background worker can flag rows where
--            now - last_event_observed_at > 30 days AND no update
--            has landed since.
--
-- Properties verified post-migration:
--   1. `declarations.last_event_observed_at` is a NULLABLE timestamptz.
--      NULL = historical row (pre-FATF migration) or row whose
--      declarant did not assert the event date. The staleness worker
--      ignores NULL rows.
--   2. Index supports the staleness worker's "find rows older than
--      threshold" query in O(log n) — without the index the worker
--      would table-scan the projection on every tick.
--   3. CHECK constraint: last_event_observed_at MUST NOT be in the
--      future. The aggregate validates this on Submit/Amend; the DB
--      CHECK is defence-in-depth against a future infrastructure
--      bypass.

BEGIN;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS last_event_observed_at TIMESTAMPTZ NULL;

-- The aggregate refuses Submit/Amend commands whose
-- last_event_observed_at is in the future relative to submitted_at;
-- the DB CHECK runs against now() (slightly more conservative — a
-- replication lag of seconds could artificially trip this, so the
-- CHECK is +60s tolerant of clock drift). The constraint is NOT VALID
-- at migration time so historical NULL rows aren't punished.
ALTER TABLE declarations
    ADD CONSTRAINT declarations_event_date_not_future CHECK (
        last_event_observed_at IS NULL
        OR last_event_observed_at <= NOW() + INTERVAL '60 seconds'
    ) NOT VALID;

-- Index supports the staleness worker's pull query:
--   SELECT declaration_id, declarant_principal, entity_id
--   FROM declarations
--   WHERE last_event_observed_at IS NOT NULL
--     AND last_event_observed_at < NOW() - INTERVAL '30 days'
--     AND amended_at IS NULL OR amended_at < last_event_observed_at
-- Partial index avoids bloating with NULL rows.
CREATE INDEX IF NOT EXISTS idx_declarations_staleness
    ON declarations(last_event_observed_at)
    WHERE last_event_observed_at IS NOT NULL;

COMMENT ON COLUMN declarations.last_event_observed_at IS
    'TODO-005 closure — FATF R.24 c.24.8 fn 29. Timestamp the '
    'declarant asserts the BO change occurred. NULL on historical '
    'rows; populated for submissions made under PR-FATF-4.B and '
    'later. The staleness watcher worker flags declarations where '
    'now() - last_event_observed_at > 30 days AND no update has '
    'landed since.';

COMMIT;
