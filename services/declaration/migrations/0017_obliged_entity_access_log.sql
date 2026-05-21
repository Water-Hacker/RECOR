-- TODO-006-followup — Per-disclosure audit log for obliged-entity
-- reads of `GET /v1/declarations/{id}`.
--
-- FATF R.24 c.24.6(c) + EU 6AMLD Art. 12 require that supervised
-- entities performing CDD against the central registry are subject to
-- per-disclosure logging. The platform must be able to answer "who
-- read which BO record on which date" for any subsequent regulator
-- review.
--
-- The table is written best-effort from the read handler in a
-- separate transaction (the read itself does NOT depend on the log
-- write — the row returns regardless; a `warn!` + metric capture
-- write failure so operations can spot a degraded audit substrate).
-- COMP-2 immutability still applies — once a row is in, it is
-- append-only.
--
-- The rate-limit machinery uses this same table as a sliding-window
-- count keyed on `obliged_entity_principal`. The default cap of 1000
-- reads / day is configurable via `OBLIGED_ENTITY_READ_LIMIT_PER_DAY`.
-- When the cap is exceeded the read is refused with 429 BEFORE the
-- log write, so a rate-limited caller does not pollute the log with
-- their refused-attempts.
--
-- The `idempotency_key` column captures whatever the caller presented
-- in `Idempotency-Key` (if anything) so a downstream forensic review
-- can pair the audit row with the obliged entity's own at-source
-- log; the `request_correlation_id` matches what the service emitted
-- on its `x-request-id` response header so cross-system traces can be
-- joined.

BEGIN;

CREATE TABLE IF NOT EXISTS obliged_entity_access_log (
    log_id                    UUID PRIMARY KEY,
    obliged_entity_principal  TEXT NOT NULL,
    declaration_id            UUID NOT NULL,
    disclosed_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fields_disclosed          JSONB NOT NULL,
    idempotency_key           TEXT,
    request_correlation_id    UUID
);

CREATE INDEX IF NOT EXISTS idx_obliged_entity_access_by_principal
    ON obliged_entity_access_log (obliged_entity_principal, disclosed_at);
CREATE INDEX IF NOT EXISTS idx_obliged_entity_access_by_declaration
    ON obliged_entity_access_log (declaration_id, disclosed_at);

-- COMP-2 immutability — same shape as the other audit tables.
DROP TRIGGER IF EXISTS forbid_obliged_entity_access_log_update ON obliged_entity_access_log;
CREATE TRIGGER forbid_obliged_entity_access_log_update
    BEFORE UPDATE ON obliged_entity_access_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_obliged_entity_access_log_delete ON obliged_entity_access_log;
CREATE TRIGGER forbid_obliged_entity_access_log_delete
    BEFORE DELETE ON obliged_entity_access_log
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_obliged_entity_access_log_truncate ON obliged_entity_access_log;
CREATE TRIGGER forbid_obliged_entity_access_log_truncate
    BEFORE TRUNCATE ON obliged_entity_access_log
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON obliged_entity_access_log FROM PUBLIC;

COMMIT;
