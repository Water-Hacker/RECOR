-- Migration: 0006_add_correction_columns
-- Service:   declaration
-- Sprint:    PI-1 (R-DECL-3 Amend + Correct slice)
-- Rationale: Amend/Correct land as new aggregate commands that touch
--            the projection. Amend re-projects existing columns
--            (beneficial_owners, effective_from, declarant_role) so it
--            needs no new storage — the in-place update path uses the
--            existing UPDATE clause keyed on declaration_id.
--
--            Correct introduces a new metadata field `metadata_notes`
--            on the projection: a free-form annotation the declarant
--            can attach pre-verification (typos, supporting-document
--            references). NULLable; pre-existing rows remain NULL.
--
--            Both commands stamp an `amended_at` / `corrected_at`
--            timestamp on the projection for operator-facing queries.
--            These mirror the timestamps already carried on the
--            DeclarationAmendedV1 / DeclarationCorrectedV1 events; the
--            event log remains the source of truth.
--
-- Properties verified post-migration:
--   1. Existing rows: `metadata_notes`, `amended_at`, `corrected_at`
--      all remain NULL.
--   2. Forward-only — rollback leaves the new columns harmless (they
--      retain NULL state and no application code reads them when
--      reverted).
--   3. The column ordering is preserved at the bottom of the table
--      so prior queries that name columns explicitly continue to
--      compile.
--
-- Doctrines tracked:
--   - D13 idempotency: ADD COLUMN IF NOT EXISTS is safe under repeat.
--   - D14 fail-closed: no CHECK relaxation; NULLability is the safe
--     default for back-fillable metadata.
--   - D19 reproducible everything: forward-only schema change.

BEGIN;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS metadata_notes TEXT;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS amended_at TIMESTAMPTZ;

ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS corrected_at TIMESTAMPTZ;

COMMIT;
