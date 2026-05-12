-- Migration: 0002_add_verification_state
-- Service:   declaration
-- Sprint:    PI-1 (D↔V loop relay, Phase 1)
-- Rationale: track the downstream verification state separately from the
--            aggregate's submission lifecycle. Verification outcomes
--            (from R-DECL-V-1 / Phase 2 writeback) will update this column.
--
-- Properties verified post-migration:
--   1. Every existing declaration row gets verification_state = 'not_verified'.
--   2. The new column is NOT NULL via DEFAULT-then-ALTER pattern.

BEGIN;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS verification_state TEXT NOT NULL DEFAULT 'not_verified'
        CHECK (verification_state IN ('not_verified', 'pending', 'in_verification', 'accepted', 'rejected'));

CREATE INDEX IF NOT EXISTS idx_declarations_verification_state
    ON declarations(verification_state)
    WHERE verification_state IN ('pending', 'in_verification');

COMMIT;
