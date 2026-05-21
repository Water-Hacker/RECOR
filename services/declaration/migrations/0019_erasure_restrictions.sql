-- TODO-032 — GDPR Art. 17 + 18 erasure-restriction requests.
--
-- BO data is held under FATF R.24 retention (5 years post-cessation
-- minimum); GDPR Art. 17(3)(b) carves out processing required for
-- compliance with a legal obligation; AML/CFT is such an obligation.
-- The platform therefore REFUSES erasure outright. What it does
-- support is GDPR Art. 18 ("right to restriction of processing"): the
-- data subject can require the platform to STORE the data without
-- further processing pending dispute resolution. Per Art. 18(2), every
-- subsequent disclosure of restricted data must carry an explicit
-- notice that the data subject has contested it.
--
-- Schema:
--
--   * `erasure_restriction_requests` — projection. State =
--     `restriction_active` (default on POST) or `withdrawn`
--     (data-subject reverses the restriction).
--   * `erasure_restriction_request_events` — append-only log; same
--     COMP-2 immutability triggers as the other audit tables.
--   * `declarations.restricted_at` — denormalised flag on the
--     projection so the read handler can include the
--     `restriction_notice` field per Art. 18(2) without an additional
--     join on the hot path.

BEGIN;

CREATE TABLE IF NOT EXISTS erasure_restriction_requests (
    request_id              UUID PRIMARY KEY,
    declaration_id          UUID NOT NULL,
    data_subject_principal  TEXT NOT NULL,
    reason                  TEXT NOT NULL,
    state                   TEXT NOT NULL CHECK (state IN (
        'restriction_active', 'withdrawn'
    )),
    -- Refusal kind echoes the documented refusal reason returned to
    -- the data subject. The handler always records `erasure_not_permitted`
    -- because the platform never grants Art. 17 erasure on BO data.
    refusal_kind            TEXT NOT NULL,
    submitted_at            TIMESTAMPTZ NOT NULL,
    withdrawn_at            TIMESTAMPTZ,
    aggregate_version       BIGINT NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_erasure_by_data_subject
    ON erasure_restriction_requests (data_subject_principal, submitted_at);
CREATE INDEX IF NOT EXISTS idx_erasure_by_declaration
    ON erasure_restriction_requests (declaration_id, submitted_at);

CREATE TABLE IF NOT EXISTS erasure_restriction_request_events (
    event_id            UUID PRIMARY KEY,
    request_id          UUID NOT NULL REFERENCES erasure_restriction_requests (request_id),
    event_type          TEXT NOT NULL,
    payload             JSONB NOT NULL,
    actor_principal     TEXT NOT NULL,
    occurred_at         TIMESTAMPTZ NOT NULL,
    sequence_no         BIGINT NOT NULL,
    UNIQUE (request_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_erasure_events_aggregate
    ON erasure_restriction_request_events (request_id, sequence_no);

DROP TRIGGER IF EXISTS forbid_erasure_restriction_events_update
    ON erasure_restriction_request_events;
CREATE TRIGGER forbid_erasure_restriction_events_update
    BEFORE UPDATE ON erasure_restriction_request_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_erasure_restriction_events_delete
    ON erasure_restriction_request_events;
CREATE TRIGGER forbid_erasure_restriction_events_delete
    BEFORE DELETE ON erasure_restriction_request_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_erasure_restriction_events_truncate
    ON erasure_restriction_request_events;
CREATE TRIGGER forbid_erasure_restriction_events_truncate
    BEFORE TRUNCATE ON erasure_restriction_request_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON erasure_restriction_request_events FROM PUBLIC;

-- Denormalised flag on the declarations projection. NULL when not
-- restricted; the timestamp of the most-recent active restriction
-- otherwise. The handler reading `GET /v1/declarations/{id}` uses
-- this column to include the `restriction_notice` field per
-- Art. 18(2).
ALTER TABLE declarations
    ADD COLUMN IF NOT EXISTS restricted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_declarations_restricted
    ON declarations (restricted_at) WHERE restricted_at IS NOT NULL;

COMMIT;
