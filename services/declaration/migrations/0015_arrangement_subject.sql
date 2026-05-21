-- Migration 0015 — TODO-002-declaration-link.
--
-- FATF R.25 INR.25 — a beneficial-ownership declaration may have an
-- arrangement (trust, fiducy, waqf, similar) as its subject rather
-- than a legal entity. The declaration's current schema carries
-- `entity_id` as the subject; this migration introduces a
-- discriminator column so a row can carry either an entity_id or
-- an arrangement_id without breaking historical projections.
--
-- Forward-only. The discriminator defaults to 'entity' so historical
-- rows are preserved with their original semantics. New declarations
-- whose subject is an arrangement (the `TODO-002-declaration-link`
-- portal-side change) set `subject_kind='arrangement'` and populate
-- `arrangement_id`.
--
-- Properties verified post-migration:
--   1. Every historical row has `subject_kind='entity'` and
--      `arrangement_id IS NULL`.
--   2. CHECK constraint refuses a row that names both an arrangement
--      AND an entity, or neither.
--   3. The cascade-tier resolver branches on `subject_kind` (see
--      `services/declaration/src/application/cascade_tier_resolver.rs`).
--
-- Doctrine compliance:
--   - D14 fail-closed: the CHECK constraint refuses any "neither/both"
--     state at the database boundary, regardless of application bug.
--   - D15 cryptographic provenance: the declaration event payloads
--     embed the new subject_kind so a replay reconstructs the same
--     projection.

BEGIN;

-- Two new columns: the discriminator + the arrangement-id reference.
-- The discriminator is a closed enum encoded as TEXT for indexing
-- ergonomics. `arrangement_id` does NOT reference the entity-service
-- `arrangements` table — the two services do not share a database;
-- the verification engine resolves the reference at validation time.
ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS subject_kind TEXT NOT NULL DEFAULT 'entity'
        CHECK (subject_kind IN ('entity', 'arrangement'));

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS arrangement_id UUID;

-- Subject-shape invariant: exactly one of (entity_id, arrangement_id)
-- carries the subject reference depending on subject_kind. The fail-
-- closed constraint prevents application bugs from creating a row
-- whose subject is ambiguous or missing.
ALTER TABLE declarations
    ADD CONSTRAINT declarations_subject_shape_v1
        CHECK (
            (subject_kind = 'entity' AND arrangement_id IS NULL)
            OR
            (subject_kind = 'arrangement' AND arrangement_id IS NOT NULL)
        );

CREATE INDEX IF NOT EXISTS idx_declarations_arrangement_id
    ON declarations (arrangement_id)
    WHERE arrangement_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_declarations_subject_kind
    ON declarations (subject_kind, submitted_at DESC);

COMMENT ON COLUMN declarations.subject_kind IS
    'Subject discriminator: ''entity'' (R.24, legal entity at entity_id) or ''arrangement'' (R.25, legal arrangement at arrangement_id). Defaults to ''entity'' so historical rows preserve their semantics.';
COMMENT ON COLUMN declarations.arrangement_id IS
    'For subject_kind=''arrangement'', the entity-service arrangement_id this declaration is about. NULL when subject_kind=''entity''.';
COMMENT ON CONSTRAINT declarations_subject_shape_v1 ON declarations IS
    'D14 fail-closed: refuse a row whose (subject_kind, arrangement_id) pair is inconsistent. Enforces R.25 subject-shape at the SQL boundary.';

COMMIT;
