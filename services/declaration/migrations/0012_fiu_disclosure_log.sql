-- TODO-008 — FIU disclosure log. FATF R.24 c.24.9 requires that
-- law-enforcement / FIUs have timely access to BO information; R.40
-- extends this to foreign FIUs via MLAT/Egmont. GDPR Art. 30
-- requires the controller to maintain records of processing — the
-- disclosure log is that record at the platform layer.
--
-- The log is append-only (COMP-2 immutability), per-row records WHICH
-- columns were disclosed, and is retained indefinitely. Operators do
-- NOT prune it under any retention worker (the
-- `infrastructure/retention.rs` worker only touches `outbox`).
--
-- The `subject_kind` discriminator captures whether the search hit
-- went through a person_id, a national_id, a declaration_id, or an
-- entity_id — so a downstream auditor reviewing the log can answer
-- "how did the FIU find this record" without joining against the
-- request-body archive (which we do not retain at any scope).

BEGIN;

CREATE TABLE IF NOT EXISTS fiu_disclosure_log (
    disclosure_id           UUID PRIMARY KEY,
    requesting_principal    TEXT NOT NULL,
    anif_case_reference     TEXT NOT NULL,
    justification_text      TEXT NOT NULL,
    subject_kind            TEXT NOT NULL CHECK (subject_kind IN (
        'person_id', 'national_id', 'declaration_id', 'entity_id', 'full_name'
    )),
    subject_value           TEXT NOT NULL,
    disclosed_at            TIMESTAMPTZ NOT NULL,
    -- Field-level audit: which columns of the matching projection
    -- were returned to the FIU. The columns enum is captured
    -- lazily as a JSONB array of column names so a future
    -- expansion of the response shape does not require a schema
    -- migration here.
    disclosed_columns       JSONB NOT NULL,
    -- Optional: the declaration_id that ended up in the response,
    -- when the request was a name / national_id search and the
    -- platform resolved one. NULL when the request was a direct
    -- declaration_id lookup (the value is then in `subject_value`).
    resolved_declaration_id UUID,
    -- TODO-008 R.40: MLAT cases capture the foreign FIU's identifier
    -- + the Egmont request id. NULL for ANIF-originated requests.
    mlat_foreign_fiu        TEXT,
    mlat_egmont_request_id  TEXT,
    -- Source-of-record event id — links to the COMP-2 audit-log row
    -- for cryptographic provenance.
    event_id                UUID NOT NULL UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_fiu_disclosure_by_principal
    ON fiu_disclosure_log (requesting_principal, disclosed_at);
CREATE INDEX IF NOT EXISTS idx_fiu_disclosure_by_case
    ON fiu_disclosure_log (anif_case_reference, disclosed_at);
CREATE INDEX IF NOT EXISTS idx_fiu_disclosure_by_declaration
    ON fiu_disclosure_log (resolved_declaration_id)
    WHERE resolved_declaration_id IS NOT NULL;

-- COMP-2 immutability — same shape as 0011 / 0007.
DROP TRIGGER IF EXISTS forbid_fiu_disclosure_log_update ON fiu_disclosure_log;
CREATE TRIGGER forbid_fiu_disclosure_log_update
    BEFORE UPDATE ON fiu_disclosure_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_fiu_disclosure_log_delete ON fiu_disclosure_log;
CREATE TRIGGER forbid_fiu_disclosure_log_delete
    BEFORE DELETE ON fiu_disclosure_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_fiu_disclosure_log_truncate ON fiu_disclosure_log;
CREATE TRIGGER forbid_fiu_disclosure_log_truncate
    BEFORE TRUNCATE ON fiu_disclosure_log
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON fiu_disclosure_log FROM PUBLIC;

COMMIT;
