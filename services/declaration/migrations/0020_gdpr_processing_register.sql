-- TODO-034 — Records of Processing Activities (GDPR Art. 30 register).
--
-- Article 30 of the GDPR requires every controller and processor to
-- maintain a written register of processing activities — purpose, legal
-- basis, categories of data, retention period, transfer safeguards.
-- The supervisory authority can request the register at any time;
-- failing to produce it is itself a violation.
--
-- The platform maintains the register as queryable structured data
-- (not a Word document) so:
--   1. Admin endpoints can attest the live register state.
--   2. New processing activities (e.g. when a new consumer integration
--      ships) MUST land a row here as part of the same PR as the code.
--   3. The DPO can export the register to PDF for a regulator inquiry.
--
-- COMP-2 immutability: `gdpr_processing_register_events` is the
-- append-only log. `gdpr_processing_register` is the projection; the
-- `retired_at` column lets us soft-delete a record while preserving
-- its history (the register must show prior activities, not only
-- currently-active ones).

BEGIN;

CREATE TABLE IF NOT EXISTS gdpr_processing_register (
    record_id              UUID PRIMARY KEY,
    controller             TEXT NOT NULL,
    processor              TEXT,
    purpose                TEXT NOT NULL,
    legal_basis            TEXT NOT NULL,
    data_categories        JSONB NOT NULL,
    subject_categories     JSONB NOT NULL,
    recipients             JSONB NOT NULL,
    retention_period_text  TEXT NOT NULL,
    transfer_safeguards    TEXT,
    created_at             TIMESTAMPTZ NOT NULL,
    updated_at             TIMESTAMPTZ NOT NULL,
    retired_at             TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_gdpr_register_active
    ON gdpr_processing_register (created_at) WHERE retired_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_gdpr_register_retired
    ON gdpr_processing_register (retired_at) WHERE retired_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS gdpr_processing_register_events (
    event_id            UUID PRIMARY KEY,
    record_id           UUID NOT NULL REFERENCES gdpr_processing_register (record_id),
    event_type          TEXT NOT NULL,
    payload             JSONB NOT NULL,
    actor_principal     TEXT NOT NULL,
    occurred_at         TIMESTAMPTZ NOT NULL,
    sequence_no         BIGINT NOT NULL,
    UNIQUE (record_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_gdpr_register_events_aggregate
    ON gdpr_processing_register_events (record_id, sequence_no);

DROP TRIGGER IF EXISTS forbid_gdpr_register_events_update
    ON gdpr_processing_register_events;
CREATE TRIGGER forbid_gdpr_register_events_update
    BEFORE UPDATE ON gdpr_processing_register_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_gdpr_register_events_delete
    ON gdpr_processing_register_events;
CREATE TRIGGER forbid_gdpr_register_events_delete
    BEFORE DELETE ON gdpr_processing_register_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_gdpr_register_events_truncate
    ON gdpr_processing_register_events;
CREATE TRIGGER forbid_gdpr_register_events_truncate
    BEFORE TRUNCATE ON gdpr_processing_register_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON gdpr_processing_register_events FROM PUBLIC;

COMMIT;
