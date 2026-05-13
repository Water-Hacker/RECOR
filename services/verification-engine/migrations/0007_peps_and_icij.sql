-- Migration: 0007_peps_and_icij
-- Service:   verification-engine
-- Tickets:   R-VER-3 (PEPs), R-VER-4 (ICIJ Offshore Leaks)
-- Rationale: Stage 4 PEP screening + Stage 5 adverse-media retrieval
--            indexes. Both reuse pg_trgm (loaded by 0004) and the same
--            canonical-name shape as `sanctions_persons` so the shared
--            `name_match` helper hits identical query plans.

BEGIN;

-- ─── PEPs (R-VER-3) ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS peps (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 'opensanctions_pep' for v1; commercial backups (e.g. 'refinitiv')
    -- arrive in a follow-up.
    source              TEXT NOT NULL,
    full_name_canonical TEXT NOT NULL,
    full_name_aliases   JSONB NOT NULL DEFAULT '[]'::jsonb,
    position            TEXT,
    country             CHAR(2),
    start_date          DATE,
    end_date            DATE,
    is_current          BOOLEAN NOT NULL DEFAULT FALSE,
    -- Whether this row is a confirmed PEP or a known associate.
    -- 'confirmed' → direct exposure; 'associate' → relation to a PEP.
    relationship_kind   TEXT NOT NULL DEFAULT 'confirmed'
        CHECK (relationship_kind IN ('confirmed', 'associate')),
    -- Reference to the parent PEP id when relationship_kind = 'associate'.
    parent_pep_id       UUID REFERENCES peps(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source_id           TEXT NOT NULL,
    UNIQUE (source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_peps_full_name_canonical_trgm
    ON peps USING gin (full_name_canonical gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_peps_country
    ON peps (country)
    WHERE country IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_peps_current
    ON peps (is_current)
    WHERE is_current = TRUE;

CREATE INDEX IF NOT EXISTS idx_peps_relationship
    ON peps (relationship_kind);

-- ─── ICIJ Offshore Leaks (R-VER-4) ────────────────────────────────────
-- Persons / officers / intermediaries / entities from the ICIJ leak set
-- (Panama, Paradise, Pandora, etc.). The 'node_kind' column carries
-- which CSV layer the row came from; v1 only consults rows tagged
-- 'person' or 'officer'.
CREATE TABLE IF NOT EXISTS icij_persons (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_kind           TEXT NOT NULL
        CHECK (node_kind IN ('person', 'officer', 'intermediary', 'entity')),
    source_id           TEXT NOT NULL,
    -- Source dataset within ICIJ (panama, paradise, pandora, etc.).
    source_dataset      TEXT NOT NULL,
    full_name_canonical TEXT NOT NULL,
    -- Free-text country tag from the leak set; many entries are
    -- jurisdictions or blanks. We keep raw and let the application
    -- layer interpret.
    country_raw         TEXT,
    -- Free-text role / position / company-affiliation snippet from the
    -- leak; consumed by the Inference Gateway as evidence context.
    snippet             TEXT,
    leaked_at           DATE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_dataset, source_id, node_kind)
);

CREATE INDEX IF NOT EXISTS idx_icij_full_name_canonical_trgm
    ON icij_persons USING gin (full_name_canonical gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_icij_node_kind
    ON icij_persons (node_kind);

COMMIT;
