-- Migration: 0003_audit_log_immutability
-- Service:   verification-engine
-- Sprint:    Phase 0 (COMP-2 — Audit log immutability + retention policy)
-- Author:    RÉCOR security-engineering
-- Forward-only: same one-way guarantee as the declaration mirror.
--
-- Mirror of services/declaration/migrations/0007_audit_log_immutability.sql.
-- See that file's header for the full rationale and grant model.
--
-- ╭─ Tables and the per-table policy ──────────────────────────────╮
-- │ verification_cases                                             │
-- │   * INSERT — yes (each adjudicated case is persisted once)     │
-- │   * SELECT — yes (case lookup + writeback queries)             │
-- │   * UPDATE — REFUSED (trigger raises; REVOKE strips PUBLIC).   │
-- │     Case payloads are the engine's append-only adjudication    │
-- │     record; D15 cryptographic provenance pins to them, and     │
-- │     ADR-002 (Dempster-Shafer fusion math is auditable) relies  │
-- │     on input + BPA bytes never changing post-adjudication.     │
-- │   * DELETE — REFUSED (trigger raises; REVOKE strips PUBLIC)    │
-- │   * TRUNCATE — REFUSED                                         │
-- │   * Retention: forever (D15)                                   │
-- │                                                                │
-- │ verification_outbox                                            │
-- │   * INSERT — yes (every writeback envelope writes a row)       │
-- │   * SELECT — yes (relay polling)                               │
-- │   * UPDATE — yes (relay sets dispatched_at + last_error)       │
-- │   * DELETE — yes (DLQ move + retention worker prune)           │
-- │   * TRUNCATE — REFUSED                                         │
-- │   * Retention: pruned 30 days after dispatched_at by           │
-- │                infrastructure/retention.rs                     │
-- │                                                                │
-- │ verification_outbox_dlq                                        │
-- │   * INSERT — yes (move-to-dlq when attempts exhaust)           │
-- │   * SELECT — yes (operator listing + replay lookup)            │
-- │   * UPDATE — REVOKED from PUBLIC                               │
-- │   * DELETE — yes (replay path: insert + delete atomically)     │
-- │   * TRUNCATE — REFUSED                                         │
-- │   * Retention: forever                                         │
-- │                                                                │
-- │ mock_bunec_persons                                             │
-- │   * Not in scope for COMP-2 — it is a dev/test fixture table   │
-- │     replaced by the real BUNEC adapter (R-VER-1). No grant     │
-- │     changes applied.                                           │
-- ╰────────────────────────────────────────────────────────────────╯

BEGIN;

-- ── verification_cases: immutable append-only case record ─────────────────

CREATE OR REPLACE FUNCTION verification_cases_refuse_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'verification_cases is append-only (COMP-2); % refused by trigger',
        TG_OP
    USING ERRCODE = 'insufficient_privilege';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_verification_cases_no_update ON verification_cases;
CREATE TRIGGER trg_verification_cases_no_update
    BEFORE UPDATE ON verification_cases
    FOR EACH ROW EXECUTE FUNCTION verification_cases_refuse_mutation();

DROP TRIGGER IF EXISTS trg_verification_cases_no_delete ON verification_cases;
CREATE TRIGGER trg_verification_cases_no_delete
    BEFORE DELETE ON verification_cases
    FOR EACH ROW EXECUTE FUNCTION verification_cases_refuse_mutation();

DROP TRIGGER IF EXISTS trg_verification_cases_no_truncate ON verification_cases;
CREATE TRIGGER trg_verification_cases_no_truncate
    BEFORE TRUNCATE ON verification_cases
    FOR EACH STATEMENT EXECUTE FUNCTION verification_cases_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON verification_cases FROM PUBLIC;

-- ── verification_outbox: TRUNCATE forbidden; UPDATE/DELETE retained ───────

REVOKE TRUNCATE ON verification_outbox FROM PUBLIC;

-- ── verification_outbox_dlq: UPDATE + TRUNCATE forbidden ─────────────────

REVOKE UPDATE, TRUNCATE ON verification_outbox_dlq FROM PUBLIC;

-- ── Table-level commentary ────────────────────────────────────────────────

COMMENT ON TABLE verification_cases IS
    'Append-only adjudication record. UPDATE/DELETE/TRUNCATE refused by trigger (COMP-2). Retained forever (D15 cryptographic provenance + ADR-002 auditable fusion).';
COMMENT ON TABLE verification_outbox IS
    'Writeback outbox. INSERT/SELECT/UPDATE/DELETE permitted; TRUNCATE forbidden. Dispatched rows pruned after 30 days (COMP-2).';
COMMENT ON TABLE verification_outbox_dlq IS
    'Dead-letter queue. INSERT/SELECT/DELETE permitted; UPDATE/TRUNCATE forbidden. Retained forever.';

COMMIT;
