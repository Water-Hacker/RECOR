-- Migration: 0003_pg_trgm_fuzzy
-- Service:   person-service (TODO-038, R-PERSON-FUZZY)
-- Author:    RÉCOR engineering / verification-engine-specialist
-- Rationale: v1 search was ILIKE-only — a Cameroonian declarant looking
--            for "N'GONO" against a record stored as "NGONO MARIE-CLAIRE"
--            would not match because '%N'gono%' fails the substring
--            test. FATF R.24 fuzzy-name-match expectations (and the
--            Stage 3 sanctions screening that consumes the same
--            person registry) require trigram + phonetic fallback.
--
--            This migration ships two complementary upgrades:
--              1. pg_trgm extension + a GIN trigram index on
--                 `canonical_full_name` so `name % $1` (similarity ≥
--                 default_threshold) is index-supported.
--              2. A `name_phonetic` STORED generated column populated
--                 from `soundex(canonical_full_name)`. Soundex is the
--                 Latin-alphabet phonetic family Postgres ships in the
--                 `fuzzystrmatch` extension; it survives diacritic
--                 stripping ("Ngono" and "N'gono" both reduce to N250)
--                 and is small enough to b-tree-index cheaply.
--
--            The repository `search` method consumes both: fuzzy=true
--            applies the trigram filter with `min_similarity` (default
--            0.3); the use case also matches `name_phonetic` for
--            soundex-equivalent fall-back hits.
--
-- D14 fail-closed: `CREATE EXTENSION IF NOT EXISTS` is the standard
-- Postgres-side superuser bootstrap. The dev / test image ships both
-- extensions; production must have them pre-installed (covered in
-- `docs/runbooks/postgres-bootstrap.md`).
-- D19 reproducible everything: extensions are part of the migration
-- so a fresh DB rebuilt from `cargo sqlx migrate run` arrives at the
-- same schema as production.

BEGIN;

-- ── Extensions ──────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS fuzzystrmatch;

-- ── Phonetic generated column ──────────────────────────────────────────
-- `STORED` so the value is materialised on INSERT/UPDATE and joins can
-- use a B-tree index. `soundex` is deterministic + IMMUTABLE which the
-- generator clause requires. Soundex is ASCII-folded; for already-ASCII
-- canonical names the output is stable.
ALTER TABLE persons
    ADD COLUMN IF NOT EXISTS name_phonetic TEXT
        GENERATED ALWAYS AS (soundex(canonical_full_name)) STORED;

COMMENT ON COLUMN persons.name_phonetic IS
    'Soundex(canonical_full_name). Phonetic fall-back for fuzzy search '
    'when trigram similarity ranks below min_similarity. Stored '
    'generated column; recomputed automatically on every UPDATE to '
    'canonical_full_name.';

-- ── Indexes ────────────────────────────────────────────────────────────
-- Trigram GIN index on canonical_full_name. Restricted to live rows so
-- merged-out shells do not surface in search.
CREATE INDEX IF NOT EXISTS idx_persons_canonical_full_name_trgm
    ON persons USING GIN (canonical_full_name gin_trgm_ops)
    WHERE merged_into IS NULL;

-- B-tree on the phonetic bucket. Tiny payload (4-char buckets) so the
-- index footprint is negligible compared to the GIN trigram index.
CREATE INDEX IF NOT EXISTS idx_persons_name_phonetic
    ON persons (name_phonetic)
    WHERE merged_into IS NULL;

COMMIT;
