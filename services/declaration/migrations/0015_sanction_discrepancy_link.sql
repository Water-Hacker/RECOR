-- TODO-003-followup — Link a sanction proceeding to the discrepancy
-- that triggered it. Closes the audit-trail loop required by FATF
-- R.24 c.24.13: when a discrepancy resolves with `resolution_kind =
-- sanction_imposed`, the back-office workflow opens a
-- `sanctions_proceedings` row inside the same transaction as the
-- discrepancy resolution event. The new `triggered_by_discrepancy_id`
-- column carries the FK so an auditor reviewing the sanction can
-- trace it back to the obliged-entity report that produced it.
--
-- Schema rationale:
--
-- * Nullable on purpose — most proceedings are opened directly by an
--   admin via `POST /v1/sanctions/initiate` (no discrepancy). Only the
--   discrepancy-resolved-to-sanction path populates the column.
-- * FK to `discrepancies (discrepancy_id)`: the discrepancy is the
--   parent aggregate; the sanction is the consequence. ON DELETE NO
--   ACTION because both aggregates are append-only — neither row is
--   ever deleted in normal operation.
-- * Partial index for the operator workflow that lists proceedings
--   opened from a specific discrepancy.

BEGIN;

ALTER TABLE sanctions_proceedings
    ADD COLUMN IF NOT EXISTS triggered_by_discrepancy_id UUID
        REFERENCES discrepancies (discrepancy_id);

CREATE INDEX IF NOT EXISTS idx_sanctions_triggered_by_discrepancy
    ON sanctions_proceedings (triggered_by_discrepancy_id)
    WHERE triggered_by_discrepancy_id IS NOT NULL;

-- Triage projection columns. Migration 0011 only carried the
-- submitted-side fields; the back-office workflow needs priority,
-- assignee, and the triage-event timestamp on the row so the queue
-- read in `GET /v1/internal/discrepancies/queue` can do a single-row
-- read instead of replaying events.
ALTER TABLE discrepancies
    ADD COLUMN IF NOT EXISTS triage_priority TEXT
        CHECK (triage_priority IS NULL
            OR triage_priority IN ('low', 'normal', 'high'));
ALTER TABLE discrepancies
    ADD COLUMN IF NOT EXISTS assignee_principal TEXT;
ALTER TABLE discrepancies
    ADD COLUMN IF NOT EXISTS triaged_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_discrepancies_queue
    ON discrepancies (triage_priority, state, submitted_at);

COMMIT;
