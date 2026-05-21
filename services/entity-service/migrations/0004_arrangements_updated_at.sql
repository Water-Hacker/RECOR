-- Migration 0004 — TODO-002-domain follow-up to 0003.
--
-- 0003 created the `arrangements` projection with DEFAULT NOW() on both
-- created_at and updated_at, but no trigger to refresh updated_at on
-- UPDATE. This migration adds the trigger (mirroring entities) so the
-- back-office can dashboard "arrangements that have not been touched in
-- N days" without joining against the event log.
--
-- Also: a partial index on `dissolution_date IS NULL` to keep the
-- "active arrangements" lookup sub-millisecond (staleness watcher
-- consumes this).
--
-- Forward-only; no down. The trigger is idempotent (CREATE OR REPLACE
-- on the function; DROP TRIGGER IF EXISTS before CREATE TRIGGER).

BEGIN;

CREATE OR REPLACE FUNCTION arrangements_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_arrangements_updated_at ON arrangements;
CREATE TRIGGER trg_arrangements_updated_at
    BEFORE UPDATE ON arrangements
    FOR EACH ROW
    EXECUTE FUNCTION arrangements_set_updated_at();

-- Staleness watcher consults `updated_at` to surface arrangements whose
-- last observed event predates the freshness threshold. The partial
-- index covers the predominant lookup pattern: "every active
-- arrangement whose updated_at is older than NOW() - threshold".
CREATE INDEX IF NOT EXISTS idx_arrangements_active_updated_at
    ON arrangements (updated_at)
    WHERE dissolution_date IS NULL;

COMMENT ON TRIGGER trg_arrangements_updated_at ON arrangements IS
    'Auto-refresh updated_at on every UPDATE so the staleness watcher and back-office dashboards see real edit times rather than the original DEFAULT NOW().';

COMMIT;
