-- Migration: 0003_add_verification_outcome_columns
-- Service:   declaration
-- Sprint:    PI-1 (D↔V loop, Phase 2 — writeback)
-- Rationale: when the Verification Engine returns a lane decision, the
--            Declaration aggregate transitions verification_state AND
--            records WHICH case produced the decision and WHEN. These
--            three columns are the projection of the new
--            `declaration.verified.v1` event.
--
-- Properties verified post-migration:
--   1. All three columns are nullable — pre-existing rows have not yet
--      been verified, and re-running migrations is idempotent.
--   2. `verification_lane` is bounded to the three lane values that
--      match `verification_cases.lane` over in the Verification engine.
--   3. Partial index on (verification_case_id) allows the writeback
--      endpoint to detect replays cheaply (idempotency: same case_id
--      arriving twice is a no-op).

BEGIN;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS verification_lane TEXT
        CHECK (verification_lane IN ('green', 'yellow', 'red'));

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS verification_case_id UUID;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS uq_declarations_verification_case_id
    ON declarations(verification_case_id)
    WHERE verification_case_id IS NOT NULL;

COMMIT;
