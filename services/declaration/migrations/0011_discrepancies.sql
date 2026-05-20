-- TODO-003 — Discrepancy reporting intake (FATF R.24 c.24.6(c) +
-- 6AMLD Art. 10). Obliged entities (banks, notaries, DNFBPs) MUST
-- be able to report a divergence between the BO information they
-- hold from their own CDD procedures and the BO information in this
-- registry. The back-office triages and resolves; the declarant is
-- notified and either corrects or is sanctioned (TODO-004).
--
-- Schema notes:
--
-- * The aggregate is event-sourced (the same shape as `declarations`
--   and `declaration_events`). `discrepancy_events` is the
--   append-only log; `discrepancies` is the projection.
-- * The projection carries the resolved state so the
--   `GET /v1/discrepancies/by-obliged-entity` endpoint can do a
--   single-row read instead of replaying events.
-- * COMP-2 immutability: the `discrepancy_events` table is
--   protected by the same BEFORE-UPDATE/DELETE/TRUNCATE trigger
--   pattern as `declaration_events`. The trigger uses the shared
--   `declaration_events_refuse_mutation()` function created in 0007.
-- * The `field_path` column stores a JSON Pointer (RFC 6901) into
--   the canonical declaration body — `/beneficial_owners/0/cascade_tier`
--   etc. — so a report is unambiguous about which assertion it
--   contests.
-- * `evidence_attachment_hash` is a BLAKE3 hex digest of the
--   evidence payload the obliged entity holds (KYC document scan,
--   correspondent-bank statement). The platform never stores the
--   evidence itself — the obliged entity must be able to produce
--   the bytes on request from the back-office investigator.

BEGIN;

CREATE TABLE IF NOT EXISTS discrepancies (
    discrepancy_id              UUID PRIMARY KEY,
    declaration_id              UUID NOT NULL,
    submitter_obliged_entity_id TEXT NOT NULL,
    submitter_principal         TEXT NOT NULL,
    field_path                  TEXT NOT NULL,
    observed_value              JSONB NOT NULL,
    expected_value              JSONB NOT NULL,
    evidence_attachment_hash    TEXT,
    state                       TEXT NOT NULL CHECK (state IN (
        'submitted', 'triaged', 'declarant_corrected',
        'discrepancy_invalid', 'sanction_imposed', 'escalated'
    )),
    submitted_at                TIMESTAMPTZ NOT NULL,
    resolved_at                 TIMESTAMPTZ,
    resolution_kind             TEXT CHECK (resolution_kind IN (
        'declarant_corrected', 'discrepancy_invalid',
        'sanction_imposed', 'escalated'
    )),
    resolution_notes            TEXT,
    aggregate_version           BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_discrepancies_declaration
    ON discrepancies (declaration_id, submitted_at);
CREATE INDEX IF NOT EXISTS idx_discrepancies_submitter
    ON discrepancies (submitter_obliged_entity_id, submitted_at);
CREATE INDEX IF NOT EXISTS idx_discrepancies_state
    ON discrepancies (state) WHERE state = 'submitted';

CREATE TABLE IF NOT EXISTS discrepancy_events (
    event_id        UUID PRIMARY KEY,
    discrepancy_id  UUID NOT NULL REFERENCES discrepancies (discrepancy_id),
    event_type      TEXT NOT NULL,
    payload         JSONB NOT NULL,
    actor_principal TEXT NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL,
    sequence_no     BIGINT NOT NULL,
    UNIQUE (discrepancy_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_discrepancy_events_aggregate
    ON discrepancy_events (discrepancy_id, sequence_no);

-- COMP-2 immutability — same shape as 0007 applied to declaration_events.
-- The shared trigger function `declaration_events_refuse_mutation()` raises EXCEPTION
-- on UPDATE/DELETE/TRUNCATE. We attach it here to the new audit table.
DROP TRIGGER IF EXISTS forbid_discrepancy_events_update ON discrepancy_events;
CREATE TRIGGER forbid_discrepancy_events_update
    BEFORE UPDATE ON discrepancy_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_discrepancy_events_delete ON discrepancy_events;
CREATE TRIGGER forbid_discrepancy_events_delete
    BEFORE DELETE ON discrepancy_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_discrepancy_events_truncate ON discrepancy_events;
CREATE TRIGGER forbid_discrepancy_events_truncate
    BEFORE TRUNCATE ON discrepancy_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON discrepancy_events FROM PUBLIC;

COMMIT;
