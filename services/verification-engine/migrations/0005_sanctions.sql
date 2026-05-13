-- Migration: 0005_sanctions
-- Service:   verification-engine
-- Ticket:    R-VER-2
-- Rationale: Stage 3 sanctions screening index. Stores normalised
--            persons from OFAC SDN, UN consolidated, and EU CFSP feeds.
--            Stage 3 queries with pg_trgm similarity on
--            full_name_canonical and Levenshtein on aliases for
--            transliterated names.

BEGIN;

-- pg_trgm provides trigram similarity + GIN index support. Loaded once
-- here; the PEP migration (0006) reuses the same extension.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE IF NOT EXISTS sanctions_persons (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Authoritative source list this row originated from.
    -- 'ofac_sdn' | 'un_consolidated' | 'eu_cfsp'
    source              TEXT NOT NULL CHECK (source IN ('ofac_sdn', 'un_consolidated', 'eu_cfsp')),
    -- Canonicalised primary name: lowercase, diacritics folded, whitespace collapsed.
    full_name_canonical TEXT NOT NULL,
    -- Aliases / aka entries from the source feed, as a JSON array of
    -- canonicalised strings. Searched via JSONB containment for
    -- transliteration fallback.
    full_name_aliases   JSONB NOT NULL DEFAULT '[]'::jsonb,
    nationality         CHAR(2),
    date_of_birth       DATE,
    sanction_program    TEXT NOT NULL,
    list_entry_date     DATE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Source-side foreign key (e.g. OFAC SDN entity number). Combined
    -- with `source` it is the upsert key for the ingest binary.
    source_id           TEXT NOT NULL,
    UNIQUE (source, source_id)
);

-- GIN trigram index on the canonical name — supports
-- `WHERE full_name_canonical % $1` queries with default 0.3 similarity
-- threshold (we filter to 0.5 in the application layer).
CREATE INDEX IF NOT EXISTS idx_sanctions_full_name_canonical_trgm
    ON sanctions_persons USING gin (full_name_canonical gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_sanctions_nationality
    ON sanctions_persons (nationality)
    WHERE nationality IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_sanctions_source
    ON sanctions_persons (source);

COMMIT;
