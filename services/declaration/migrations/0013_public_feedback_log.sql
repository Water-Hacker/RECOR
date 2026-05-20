-- TODO-009 — Public-feedback log. FATF R.24 Guidance §3.5 + 6AMLD
-- Art. 10 + Open Ownership Principle 5.5: a public BO register MUST
-- accept and triage feedback from civil society. Post-Sovim, public
-- access is conditioned on the registry being "necessary and
-- proportionate" — accepting public feedback is part of that
-- justification (the public is being asked to verify, not just read).
--
-- Schema mirrors `discrepancies` from migration 0011 with two
-- differences: (a) no `submitter_obliged_entity_id` (submitter is
-- public — pseudonymous), and (b) `captcha_token_hash` records a
-- BLAKE3 digest of the verified CAPTCHA token so an auditor can
-- prove the row passed the rate-limit gate. Per-IP rate limiting is
-- a runtime concern enforced by the handler, not by this schema.
--
-- Mass-flag handling: when more than N reports name the same
-- declaration_id within a configurable window, the row's
-- `triage_priority` is set to `low` (anonymous mass-flag); the back-
-- office workflow surfaces these as a batch.

BEGIN;

CREATE TABLE IF NOT EXISTS public_feedback_log (
    feedback_id           UUID PRIMARY KEY,
    declaration_id        UUID,
    entity_id             UUID,
    submitter_contact     TEXT,
    -- Hash of the CAPTCHA token validated by the handler. Stored so a
    -- subsequent audit can verify the row passed the gate without
    -- retaining the raw token (D18 — no secrets in the log).
    captcha_token_hash    TEXT,
    submitter_ip_hash     TEXT,
    description           TEXT NOT NULL,
    evidence_url          TEXT,
    triage_priority       TEXT NOT NULL CHECK (triage_priority IN (
        'low', 'normal', 'high'
    )) DEFAULT 'normal',
    state                 TEXT NOT NULL CHECK (state IN (
        'submitted', 'triaged', 'resolved', 'dismissed'
    )) DEFAULT 'submitted',
    submitted_at          TIMESTAMPTZ NOT NULL,
    resolved_at           TIMESTAMPTZ,
    resolution_notes      TEXT,
    -- At least one of declaration_id / entity_id MUST be present.
    CONSTRAINT public_feedback_target_present
        CHECK (declaration_id IS NOT NULL OR entity_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_public_feedback_declaration
    ON public_feedback_log (declaration_id, submitted_at)
    WHERE declaration_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_public_feedback_entity
    ON public_feedback_log (entity_id, submitted_at)
    WHERE entity_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_public_feedback_state
    ON public_feedback_log (state, submitted_at)
    WHERE state = 'submitted';
CREATE INDEX IF NOT EXISTS idx_public_feedback_ip
    ON public_feedback_log (submitter_ip_hash, submitted_at)
    WHERE submitter_ip_hash IS NOT NULL;

-- COMP-2 immutability — same shape.
DROP TRIGGER IF EXISTS forbid_public_feedback_log_update ON public_feedback_log;
CREATE TRIGGER forbid_public_feedback_log_update
    BEFORE UPDATE ON public_feedback_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_public_feedback_log_delete ON public_feedback_log;
CREATE TRIGGER forbid_public_feedback_log_delete
    BEFORE DELETE ON public_feedback_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_public_feedback_log_truncate ON public_feedback_log;
CREATE TRIGGER forbid_public_feedback_log_truncate
    BEFORE TRUNCATE ON public_feedback_log
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON public_feedback_log FROM PUBLIC;

COMMIT;
