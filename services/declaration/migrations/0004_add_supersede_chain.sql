-- Migration: 0004_add_supersede_chain
-- Service:   declaration
-- Sprint:    PI-1 (Lifecycle completeness, R-DECL-3 slice)
-- Rationale: a beneficial-ownership declaration is not a one-shot
--            event. Entities update their ownership over time
--            (acquisitions, restructurings, annual renewals). We
--            represent this as a chain of declarations: each new
--            declaration may reference the previous one it replaces.
--            The OLD declaration's state transitions to Superseded;
--            the NEW declaration starts fresh.
--
--            This migration adds the projection columns. The event
--            log carries `declaration.superseded.v1` (emitted against
--            the OLD aggregate) and `declaration.submitted.v1` (the
--            new aggregate, unchanged) — supersede semantics live in
--            the aggregate, not in a schema change to events.
--
--            INVARIANT enforced by aggregate (not by DB):
--              - A declaration can be superseded only once
--                (superseded_by_declaration_id is set at most once
--                in the aggregate's lifetime).
--              - The superseding declaration's `supersedes_declaration_id`
--                points to a declaration in state Accepted or
--                InVerification (set at submit time).
--
-- Properties verified post-migration:
--   1. Existing rows: both columns remain NULL (no historical
--      supersede chains).
--   2. Foreign-key not enforced at the DB level — superseding
--      declarations may target ids in other shards in future.
--      Application enforces referential integrity inside its own
--      shard.

BEGIN;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS supersedes_declaration_id UUID;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS superseded_by_declaration_id UUID;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS superseded_at TIMESTAMPTZ;

-- A declaration can be superseded by at most one successor.
CREATE UNIQUE INDEX IF NOT EXISTS uq_declarations_superseded_by
    ON declarations(superseded_by_declaration_id)
    WHERE superseded_by_declaration_id IS NOT NULL;

-- Forward-chain lookup (find the successor of a given declaration).
CREATE INDEX IF NOT EXISTS idx_declarations_supersedes
    ON declarations(supersedes_declaration_id)
    WHERE supersedes_declaration_id IS NOT NULL;

-- Extend the `state` CHECK constraint to admit 'superseded'. Postgres
-- doesn't let us ALTER an inline CHECK in-place; drop + recreate.
ALTER TABLE declarations
    DROP CONSTRAINT IF EXISTS declarations_state_check;

ALTER TABLE declarations
    ADD CONSTRAINT declarations_state_check
        CHECK (state IN ('draft', 'submitted', 'in_verification',
                         'accepted', 'rejected', 'superseded'));

COMMIT;
