-- TODO-004-appeals — Entity-facing appeal-submission surface for the
-- sanctions ladder. FATF R.24 c.24.13 "proportionate, dissuasive,
-- effective" sanctions imply the targeted entity has a route to
-- contest a listing (due-process minimum); ADR-0012 explicitly
-- flagged this as a v1 follow-up gap. This migration adds the data
-- substrate.
--
-- Workflow:
--
--   1. The declarant whose declaration is targeted by a sanction
--      proceeding posts an appeal via
--      `POST /v1/sanctions/{proceeding_id}/appeal`. The handler
--      validates that the calling principal owns the targeted
--      declaration (zero-trust — never trusts the request body).
--   2. The back-office reviews and admin-resolves via
--      `POST /v1/internal/sanction-appeals/{id}/resolve` with an
--      `upheld` / `denied` outcome. When upheld, the parent
--      proceeding is transitioned to `withdrawn` in the SAME
--      transaction so the public list reflects the resolution at
--      once.
--   3. Every transition lands in `sanction_appeal_events` (COMP-2
--      immutable). The aggregate_version on `sanction_appeals`
--      gives optimistic-concurrency control parity with
--      `sanctions_proceedings`.
--
-- Notification fan-out: when a proceeding reaches `public_listed` or
-- `fined`, the back-office workflow needs to notify the entity owner
-- so they can lodge an appeal. The `sanction_notifications` table is
-- the contract between the declaration service (writer) and the
-- back-office notification consumer (out-of-band reader). The
-- consumer marks rows as sent by stamping `sent_at`.

BEGIN;

CREATE TABLE IF NOT EXISTS sanction_appeals (
    appeal_id            UUID PRIMARY KEY,
    proceeding_id        UUID NOT NULL REFERENCES sanctions_proceedings (proceeding_id),
    appellant_principal  TEXT NOT NULL,
    appeal_text          TEXT NOT NULL,
    evidence_url         TEXT,
    submitted_at         TIMESTAMPTZ NOT NULL,
    state                TEXT NOT NULL CHECK (state IN (
        'submitted', 'upheld', 'denied'
    )),
    resolved_at          TIMESTAMPTZ,
    resolved_by          TEXT,
    resolution_notes     TEXT,
    aggregate_version    BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sanction_appeals_proceeding
    ON sanction_appeals (proceeding_id, submitted_at);
CREATE INDEX IF NOT EXISTS idx_sanction_appeals_appellant
    ON sanction_appeals (appellant_principal, submitted_at);
CREATE INDEX IF NOT EXISTS idx_sanction_appeals_state_submitted
    ON sanction_appeals (state) WHERE state = 'submitted';

CREATE TABLE IF NOT EXISTS sanction_appeal_events (
    event_id        UUID PRIMARY KEY,
    appeal_id       UUID NOT NULL REFERENCES sanction_appeals (appeal_id),
    event_type      TEXT NOT NULL,
    payload         JSONB NOT NULL,
    actor_principal TEXT NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL,
    sequence_no     BIGINT NOT NULL,
    UNIQUE (appeal_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_sanction_appeal_events_aggregate
    ON sanction_appeal_events (appeal_id, sequence_no);

-- COMP-2 immutability on the events table.
DROP TRIGGER IF EXISTS forbid_sanction_appeal_events_update ON sanction_appeal_events;
CREATE TRIGGER forbid_sanction_appeal_events_update
    BEFORE UPDATE ON sanction_appeal_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanction_appeal_events_delete ON sanction_appeal_events;
CREATE TRIGGER forbid_sanction_appeal_events_delete
    BEFORE DELETE ON sanction_appeal_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanction_appeal_events_truncate ON sanction_appeal_events;
CREATE TRIGGER forbid_sanction_appeal_events_truncate
    BEFORE TRUNCATE ON sanction_appeal_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON sanction_appeal_events FROM PUBLIC;

-- Notification fan-out queue. Written by the declaration service when
-- a proceeding reaches `public_listed` or `fined`; consumed
-- out-of-band by the back-office notification worker which sets
-- `sent_at` after delivery (email, SMS, registered post).
CREATE TABLE IF NOT EXISTS sanction_notifications (
    notification_id      UUID PRIMARY KEY,
    proceeding_id        UUID NOT NULL REFERENCES sanctions_proceedings (proceeding_id),
    recipient_principal  TEXT NOT NULL,
    notification_kind    TEXT NOT NULL,
    payload              JSONB NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at              TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_sanction_notifications_unsent
    ON sanction_notifications (created_at) WHERE sent_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_sanction_notifications_proceeding
    ON sanction_notifications (proceeding_id, created_at);

COMMIT;
