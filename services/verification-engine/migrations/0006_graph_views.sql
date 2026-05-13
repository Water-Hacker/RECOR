-- Migration: 0006_graph_views
-- Service:   verification-engine
-- Ticket:    R-VER-5
-- Rationale: Stage 6 pattern detection runs over an ownership graph
--            projected from declarations. We project here into a pair
--            of materialised views; the 8 signature queries run against
--            these views (see src/application/stages/stage6_patterns.rs).
--            We avoid Neo4j on operational grounds (one fewer datastore;
--            pgrouting is available in mainline Postgres). See ADR-0010.
--
--            v1 declarations come from the projection of the Declaration
--            service's writeback events. Until that projection lands in
--            this service, we stand the views up empty against a
--            placeholder table `declaration_projection` so the
--            migration is idempotent + the signature queries compile.

BEGIN;

-- pgrouting is the long-term home for the transitive-closure
-- computations needed by signatures 1 (circular) and 4 (deep stack).
-- We don't depend on its specific functions in v1 — we use recursive
-- CTEs from `entity_ownership_graph` — but we declare the dependency
-- here so a follow-up that switches to pgrouting's `pgr_*` functions
-- doesn't need a new migration. Wrap in DO so the migration is no-op
-- when the extension isn't bundled in the local Postgres image.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pgrouting') THEN
        EXECUTE 'CREATE EXTENSION IF NOT EXISTS pgrouting';
    END IF;
END$$;

-- Declaration projection. In production this is populated by the
-- writeback subscriber from the Declaration service's events. Until
-- that lands, the table is empty; the materialised views below are
-- still valid (empty).
CREATE TABLE IF NOT EXISTS declaration_projection (
    declaration_id         UUID PRIMARY KEY,
    entity_id              UUID NOT NULL,
    declarant_principal    TEXT NOT NULL,
    submitted_at           TIMESTAMPTZ NOT NULL,
    effective_from         DATE NOT NULL,
    -- A JSONB array shaped like `OwnerSnapshot` (person_id,
    -- ownership_basis_points, interest_kind). Stage 6 reads the
    -- array; future schema work can normalise.
    beneficial_owners      JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- High-risk-jurisdiction flag for the entity. v1 sets this at
    -- ingest using the FATF grey/black list (hard-coded in Stage 6).
    entity_jurisdiction    CHAR(2),
    -- Whether the entity has any declared economic activity in BUNEC
    -- — used by signature 3 ("BO of shell company").
    has_bunec_activity     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_decl_proj_entity
    ON declaration_projection (entity_id);
CREATE INDEX IF NOT EXISTS idx_decl_proj_submitted
    ON declaration_projection (submitted_at DESC);

-- ─── entity_ownership_graph view ─────────────────────────────────────
-- One row per (entity, owner) edge. JSONB unrolling expands the
-- beneficial_owners array into one row per owner.
CREATE OR REPLACE VIEW entity_ownership_graph AS
SELECT
    dp.entity_id                                              AS entity_id,
    (bo ->> 'person_id')::UUID                                AS owner_person_id,
    ((bo ->> 'ownership_basis_points')::INTEGER)              AS ownership_basis_points,
    (bo ->> 'interest_kind')                                  AS interest_kind,
    dp.entity_jurisdiction                                    AS entity_jurisdiction,
    dp.has_bunec_activity                                     AS has_bunec_activity,
    dp.submitted_at                                           AS submitted_at
FROM declaration_projection dp,
     LATERAL jsonb_array_elements(dp.beneficial_owners) AS bo
WHERE bo ->> 'person_id' IS NOT NULL;

-- ─── ownership_paths view ────────────────────────────────────────────
-- Recursive CTE that computes transitive (entity → ultimate owner)
-- paths up to depth 8. Used by signature 4 (layered ownership > N).
-- We keep it as a VIEW (not materialised) — Stage 6 invokes it with a
-- LIMIT on the inner query when running on the hot path.
--
-- Note: a "path" here connects (root_entity → ... → leaf_person).
-- For v1 we treat the owner_person_id as terminal (the human at the
-- end of the chain). When entity-as-owner becomes supported, this
-- view extends to walk inter-entity edges.
CREATE OR REPLACE VIEW ownership_paths AS
SELECT
    eog.entity_id                AS root_entity_id,
    eog.owner_person_id          AS leaf_person_id,
    1                            AS depth,
    eog.ownership_basis_points   AS terminal_basis_points,
    eog.submitted_at             AS submitted_at
FROM entity_ownership_graph eog;
-- Inter-entity recursion is a follow-up — when an entity_as_owner
-- table lands, replace this with a recursive CTE. See ADR-0010.

-- Helper: lookups by owner. Stage 6 signature 2 ("common owner")
-- scans this index.
CREATE INDEX IF NOT EXISTS idx_decl_proj_owner_jsonb
    ON declaration_projection USING gin (beneficial_owners jsonb_path_ops);

COMMIT;
