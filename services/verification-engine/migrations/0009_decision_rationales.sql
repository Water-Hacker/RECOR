-- Migration: 0009_decision_rationales
-- Service:   verification-engine
-- TODO-049 — Per-decision explainability event (procedural-fairness gap).
--
-- Every adjudicated verification case carries an immutable rationale
-- record persisted in the SAME transaction as the case row. The
-- rationale is the load-bearing artefact for procedural-fairness
-- defensibility: declarants, analysts, and external oversight bodies
-- can read the structured chain of reasoning behind a lane decision
-- and reason about whether it was correct.
--
-- ╭─ Table policy (COMP-2 mirror of verification_cases) ───────────╮
-- │ decision_rationales                                            │
-- │   * INSERT — yes (one row per case, same transaction)          │
-- │   * SELECT — yes (declarant + admin reads)                     │
-- │   * UPDATE — REFUSED (trigger raises; REVOKE strips PUBLIC)    │
-- │   * DELETE — REFUSED (trigger raises; REVOKE strips PUBLIC)    │
-- │   * TRUNCATE — REFUSED                                         │
-- │   * Retention: forever (D15 cryptographic provenance — the     │
-- │     rationale is the explanatory anchor for the case it        │
-- │     accompanies; pruning it would create cases the registry    │
-- │     cannot explain).                                           │
-- ╰────────────────────────────────────────────────────────────────╯

BEGIN;

CREATE TABLE IF NOT EXISTS decision_rationales (
    case_id            UUID PRIMARY KEY REFERENCES verification_cases(case_id),
    declaration_id     UUID NOT NULL,
    rationale_payload  JSONB NOT NULL,
    composed_at        TIMESTAMPTZ NOT NULL
);

-- Operator queries: read by declaration_id to recover the rationale
-- chain alongside the case projection. PK on case_id is the primary
-- access path; this secondary index supports the declarant-side
-- writeback view.
CREATE INDEX IF NOT EXISTS idx_decision_rationales_declaration_id
    ON decision_rationales (declaration_id);

-- COMP-2 immutability triggers — same shape as verification_cases.
CREATE OR REPLACE FUNCTION decision_rationales_refuse_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'decision_rationales is append-only (COMP-2 / TODO-049); % refused by trigger',
        TG_OP
    USING ERRCODE = 'insufficient_privilege';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_decision_rationales_no_update ON decision_rationales;
CREATE TRIGGER trg_decision_rationales_no_update
    BEFORE UPDATE ON decision_rationales
    FOR EACH ROW EXECUTE FUNCTION decision_rationales_refuse_mutation();

DROP TRIGGER IF EXISTS trg_decision_rationales_no_delete ON decision_rationales;
CREATE TRIGGER trg_decision_rationales_no_delete
    BEFORE DELETE ON decision_rationales
    FOR EACH ROW EXECUTE FUNCTION decision_rationales_refuse_mutation();

DROP TRIGGER IF EXISTS trg_decision_rationales_no_truncate ON decision_rationales;
CREATE TRIGGER trg_decision_rationales_no_truncate
    BEFORE TRUNCATE ON decision_rationales
    FOR EACH STATEMENT EXECUTE FUNCTION decision_rationales_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON decision_rationales FROM PUBLIC;

COMMENT ON TABLE decision_rationales IS
    'Per-case explainability record (TODO-049). Append-only; UPDATE/DELETE/TRUNCATE refused by trigger (COMP-2). One row per verification case, persisted in the same transaction. Retained forever (D15).';

COMMIT;
