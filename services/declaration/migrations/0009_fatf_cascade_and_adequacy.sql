-- Migration: 0009_fatf_cascade_and_adequacy
-- Service:   declaration
-- Sprint:    PR-FATF-2.A (FATF-readiness Pass 2 — domain layer)
-- Author:    RÉCOR engineering
-- Rationale: closes TODO-001 (FATF cascade tier + control basis on every
--            BO), TODO-010 (bearer-share + nominee structural fields on
--            the BO row), and TODO-021 (explicit `adequacy_claims` block
--            asserting adequate/accurate/up-to-date).
--
-- The actual cascade + nominee fields live INSIDE the `beneficial_owners`
-- JSONB column — they are per-owner, not per-declaration. The aggregate
-- enforces them at the domain layer; this migration only documents and
-- indexes what the projection writes. The adequacy_claims block is
-- per-declaration and gets its own column.
--
-- Properties verified post-migration:
--   1. `declarations.adequacy_claims` is a nullable JSONB column.
--      NULL = historical / legacy / not-yet-set; the API DTO will
--      refuse new POSTs without it once PR-FATF-2.B ships.
--   2. Index on cascade_tier extracted from the JSONB owners array
--      so queries like "show me all declarations carrying an SMO BO"
--      remain index-supported as the table scales.
--   3. The audit event log (`declaration_events`) is unaltered —
--      historical events deserialise via the Rust `#[serde(default)]`
--      machinery; events written from this migration onward carry the
--      new fields verbatim inside `event_payload`.
--   4. No COMP-2 trigger changes — append-only invariants on
--      `declaration_events` already hold; the new fields just appear
--      inside the immutable payload bytes.

BEGIN;

-- ── declarations: adequacy_claims block ─────────────────────────────
--
-- Nullable for back-compat: historical rows have NULL; new rows
-- written under PR-FATF-2.B onward have a JSON object shape:
--   {
--     "adequate": true,
--     "accurate": true,
--     "up_to_date_as_of": "2026-04-22T10:00:00Z",
--     "legal_basis": "CEMAC AML/CFT règlement art. 12"
--   }
ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS adequacy_claims JSONB NULL;

-- Provenance: the back-office sanctions workflow (TODO-004) reads
-- claims.legal_basis to apply proportionate sanctions on perjury. An
-- index on `(adequacy_claims->>'legal_basis')` is premature (cardinality
-- unknown at v1); add it under TODO-004's PR if it materialises.

-- ── Optional cascade-tier convenience index ─────────────────────────
--
-- Most cascade-tier queries today are "find declarations with at least
-- one BO at tier X" — a JSONB GIN index on the owners array makes
-- those queries cheap. We add it on the path-expression for the tier
-- field specifically to avoid bloating GIN with every owner-row JSON.
--
-- The expression `jsonb_path_query_array(beneficial_owners, '$[*].cascade_tier')`
-- materialises a small JSONB array per row that GIN can index efficiently.
CREATE INDEX IF NOT EXISTS idx_declarations_bo_cascade_tier
    ON declarations USING GIN (
        (jsonb_path_query_array(beneficial_owners, '$[*].cascade_tier'))
    );

-- ── Optional nominee-flag convenience index ─────────────────────────
CREATE INDEX IF NOT EXISTS idx_declarations_bo_nominee
    ON declarations USING GIN (
        (jsonb_path_query_array(beneficial_owners, '$[*].is_nominee'))
    );

-- ── Comment columns for forensic readability ────────────────────────
COMMENT ON COLUMN declarations.adequacy_claims IS
    'TODO-021 closure — declarant assertion that BO data is adequate, '
    'accurate, and up-to-date per FATF R.24 c.24.8. Nullable on '
    'historical rows; required for new submissions under PR-FATF-2.B.';

COMMIT;
