-- Migration: 0007_audit_log_immutability
-- Service:   declaration
-- Sprint:    Phase 0 (COMP-2 — Audit log immutability + retention policy)
-- Author:    RÉCOR security-engineering
-- Forward-only: this migration is intentionally one-way. There is no
-- "down" path for an immutability guarantee — once the event log is
-- locked, unlocking it requires a manual operator intervention with a
-- procedural audit trail, not a code-driven rollback.
--
-- ╭─ Rationale ────────────────────────────────────────────────────╮
-- │ Doctrine 15 (cryptographic provenance) requires that every     │
-- │ consequential event the platform records be tamper-evident.    │
-- │ The `declaration_events` table is the source of truth for      │
-- │ every state-changing operation on a declaration; receipts      │
-- │ (BLAKE3 over the canonical declaration payload) pin to rows    │
-- │ in this table.                                                 │
-- │                                                                │
-- │ The application code in `infrastructure/postgres.rs` never     │
-- │ issues UPDATE or DELETE against `declaration_events` — the     │
-- │ only access pattern is INSERT (new event) + SELECT (replay,    │
-- │ projection rebuild). This migration enforces that contract at  │
-- │ the SQL boundary so a future bug, a compromised connection, or │
-- │ a misguided DBA cannot silently rewrite history.               │
-- │                                                                │
-- │ Threat-model reference (docs/security/threat-model.md):        │
-- │   * STRIDE-T: "Tampered event log after write" — closes        │
-- │     the in-application-code surface;                           │
-- │   * Gap G1 ("no in-DB audit chain") — partially closed; full   │
-- │     closure requires R-DECL-9 (Fabric audit-channel anchoring) │
-- │     which is a separate ticket.                                │
-- ╰────────────────────────────────────────────────────────────────╯
--
-- ╭─ Grant model ──────────────────────────────────────────────────╮
-- │ The Declaration service connects to Postgres as the user       │
-- │ supplied via DATABASE_URL. In every shipped configuration this │
-- │ user is `recor` (see services/declaration/docker-compose.yaml  │
-- │ and the production deployment manifests). In testcontainers-   │
-- │ backed integration tests the user is `postgres` (the image's   │
-- │ default superuser). In production the operator MAY further     │
-- │ split the role into a separate `recor_app` role that is NOT    │
-- │ the table owner (so REVOKEs bind against it); the migration    │
-- │ below is written to be correct in BOTH layouts.                │
-- │                                                                │
-- │ Layered defence:                                               │
-- │  1. REVOKE UPDATE, DELETE, TRUNCATE FROM PUBLIC and from any   │
-- │     non-owner role discovered at migration time. PostgreSQL    │
-- │     treats REVOKE against the table owner as a no-op (the      │
-- │     owner retains implicit ALL privileges), so REVOKE alone is │
-- │     insufficient when the migration runner is also the owner.  │
-- │  2. BEFORE UPDATE / BEFORE DELETE / BEFORE TRUNCATE triggers   │
-- │     that RAISE EXCEPTION. These fire regardless of the         │
-- │     invoking role — including the owner. The triggers are the  │
-- │     load-bearing immutability primitive; the REVOKEs are       │
-- │     belt-and-braces for the separate-app-role deployment.      │
-- ╰────────────────────────────────────────────────────────────────╯
--
-- ╭─ Tables and the per-table policy ──────────────────────────────╮
-- │ declaration_events                                             │
-- │   * INSERT — yes (the source of truth append)                  │
-- │   * SELECT — yes (replay + projection rebuild)                 │
-- │   * UPDATE — REFUSED (trigger raises; REVOKE strips PUBLIC)    │
-- │   * DELETE — REFUSED (trigger raises; REVOKE strips PUBLIC)    │
-- │   * TRUNCATE — REFUSED (trigger raises; REVOKE strips PUBLIC)  │
-- │   * Retention: forever (D15)                                   │
-- │                                                                │
-- │ outbox                                                         │
-- │   * INSERT — yes (every domain event writes a row)             │
-- │   * SELECT — yes (relay polling + admin DLQ listing)           │
-- │   * UPDATE — yes (relay sets dispatched_at, dispatch_attempts, │
-- │              last_error)                                       │
-- │   * DELETE — yes (DLQ "move-to-dlq" + the retention worker     │
-- │              prunes dispatched rows older than 30 days)        │
-- │   * TRUNCATE — REFUSED (REVOKE strips PUBLIC; truncating the   │
-- │                outbox would silently drop un-relayed events)   │
-- │   * Retention: pruned 30 days after dispatched_at by the       │
-- │                retention worker (see                           │
-- │                infrastructure/retention.rs)                    │
-- │                                                                │
-- │ outbox_dlq                                                     │
-- │   * INSERT — yes (move-to-dlq when dispatch_attempts exhausts) │
-- │   * SELECT — yes (operator listing + replay lookup)            │
-- │   * UPDATE — REVOKED from PUBLIC (the application never        │
-- │              updates DLQ rows in place; mutation would         │
-- │              indicate a real defect)                           │
-- │   * DELETE — yes (replay atomically inserts back into outbox + │
-- │              deletes from outbox_dlq in one transaction)       │
-- │   * TRUNCATE — REFUSED (same reasoning as outbox)              │
-- │   * Retention: forever (DLQ is the incident-investigation      │
-- │                surface; pruning it would lose forensic data)   │
-- ╰────────────────────────────────────────────────────────────────╯

BEGIN;

-- ── declaration_events: immutable append-only event log ────────────────────

-- Trigger function: refuse every non-INSERT mutation on the event log.
-- The function is intentionally a plain plpgsql RAISE so the error
-- propagates to the client as a SQLSTATE-bearing rejection that sqlx
-- surfaces as `sqlx::Error::Database`. The integration test in
-- `services/declaration/tests/audit_immutability.rs` asserts the
-- refusal-and-error path.
CREATE OR REPLACE FUNCTION declaration_events_refuse_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'declaration_events is append-only (COMP-2); % refused by trigger',
        TG_OP
    USING ERRCODE = 'insufficient_privilege';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_declaration_events_no_update ON declaration_events;
CREATE TRIGGER trg_declaration_events_no_update
    BEFORE UPDATE ON declaration_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_declaration_events_no_delete ON declaration_events;
CREATE TRIGGER trg_declaration_events_no_delete
    BEFORE DELETE ON declaration_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS trg_declaration_events_no_truncate ON declaration_events;
CREATE TRIGGER trg_declaration_events_no_truncate
    BEFORE TRUNCATE ON declaration_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

-- Belt-and-braces: strip UPDATE/DELETE/TRUNCATE from PUBLIC so any
-- non-owner role inherits a deny-by-default posture. In the
-- separate-app-role production deployment, this is the load-bearing
-- guarantee; the triggers above remain the catch-all.
REVOKE UPDATE, DELETE, TRUNCATE ON declaration_events FROM PUBLIC;

-- ── outbox: TRUNCATE forbidden; UPDATE/DELETE retained ────────────────────

REVOKE TRUNCATE ON outbox FROM PUBLIC;

-- ── outbox_dlq: TRUNCATE and UPDATE forbidden; INSERT/SELECT/DELETE keep ──

REVOKE UPDATE, TRUNCATE ON outbox_dlq FROM PUBLIC;

-- ── Documented commentary on the tables ───────────────────────────────────

COMMENT ON TABLE declaration_events IS
    'Append-only event log. UPDATE/DELETE/TRUNCATE refused by trigger (COMP-2). Retained forever (D15 cryptographic provenance).';
COMMENT ON TABLE outbox IS
    'Outbox for at-least-once event delivery. INSERT/SELECT/UPDATE/DELETE permitted; TRUNCATE forbidden. Dispatched rows pruned after 30 days by infrastructure/retention.rs (COMP-2).';
COMMENT ON TABLE outbox_dlq IS
    'Dead-letter queue. INSERT/SELECT/DELETE permitted; UPDATE/TRUNCATE forbidden. Retained forever (incident-investigation surface).';

COMMIT;
