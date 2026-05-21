-- TODO-032 — GDPR Art. 16 rectification requests (data-subject right).
--
-- The declarant POSTs `/v1/me/rectify` with the declaration_id, a JSON
-- Pointer (RFC 6901) into the canonical body, the requested value, and
-- a reason. The platform records the request as a tracked workflow
-- ("submitted") and queues it for the back-office. Admins ratify the
-- request via `/v1/internal/rectification-requests/{id}/approve` (state
-- → approved) or reject with a documented reason (state → rejected).
-- Withdrawn is the declarant-initiated state cancellation.
--
-- Important doctrine note (D15 cryptographic provenance): the admin
-- approval marks the request as RATIFIED in the platform's records; the
-- actual modification of the declaration still requires the declarant
-- to submit a Correct or Amend command with their own Ed25519
-- attestation. The platform NEVER signs on the declarant's behalf.
-- The `applied_correction_declaration_event_id` column records the
-- correction event id when the declarant follows through, closing the
-- audit loop.
--
-- COMP-2 immutability: `rectification_request_events` is the
-- append-only log. The shared trigger function
-- `declaration_events_refuse_mutation()` (created in 0007) is attached
-- to it.

BEGIN;

CREATE TABLE IF NOT EXISTS rectification_requests (
    request_id              UUID PRIMARY KEY,
    declaration_id          UUID NOT NULL,
    data_subject_principal  TEXT NOT NULL,
    field_path              TEXT NOT NULL,
    requested_value         JSONB NOT NULL,
    reason                  TEXT NOT NULL,
    state                   TEXT NOT NULL CHECK (state IN (
        'submitted', 'approved', 'rejected', 'withdrawn'
    )),
    submitted_at            TIMESTAMPTZ NOT NULL,
    resolved_at             TIMESTAMPTZ,
    resolver_principal      TEXT,
    resolution_notes        TEXT,
    applied_correction_event_id UUID,
    aggregate_version       BIGINT NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_rectification_by_data_subject
    ON rectification_requests (data_subject_principal, submitted_at);
CREATE INDEX IF NOT EXISTS idx_rectification_by_declaration
    ON rectification_requests (declaration_id, submitted_at);
CREATE INDEX IF NOT EXISTS idx_rectification_open
    ON rectification_requests (state) WHERE state = 'submitted';

CREATE TABLE IF NOT EXISTS rectification_request_events (
    event_id            UUID PRIMARY KEY,
    request_id          UUID NOT NULL REFERENCES rectification_requests (request_id),
    event_type          TEXT NOT NULL,
    payload             JSONB NOT NULL,
    actor_principal     TEXT NOT NULL,
    occurred_at         TIMESTAMPTZ NOT NULL,
    sequence_no         BIGINT NOT NULL,
    UNIQUE (request_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_rectification_events_aggregate
    ON rectification_request_events (request_id, sequence_no);

DROP TRIGGER IF EXISTS forbid_rectification_events_update ON rectification_request_events;
CREATE TRIGGER forbid_rectification_events_update
    BEFORE UPDATE ON rectification_request_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_rectification_events_delete ON rectification_request_events;
CREATE TRIGGER forbid_rectification_events_delete
    BEFORE DELETE ON rectification_request_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_rectification_events_truncate ON rectification_request_events;
CREATE TRIGGER forbid_rectification_events_truncate
    BEFORE TRUNCATE ON rectification_request_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON rectification_request_events FROM PUBLIC;

COMMIT;
