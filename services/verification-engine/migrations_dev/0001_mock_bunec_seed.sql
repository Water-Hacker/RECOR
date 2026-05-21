-- Migration: 0001_mock_bunec_seed (DEV-ONLY)
-- Service:   verification-engine
-- Ticket:    TODO-060
-- Rationale: dev seed for the `mock_bunec_persons` table created in
--            the production migration 0001_initial.sql. This file is
--            applied ONLY when:
--              (a) the `mock-bunec` cargo feature is enabled at
--                  compile time (production builds disable it); AND
--              (b) the operator sets `RECOR_DEV_MIGRATIONS=true` at
--                  runtime.
--
--            Both gates must hold. Production builds cannot reach
--            this SQL at all (the dev migrations directory is not
--            embedded by `sqlx::migrate!` unless the feature is on).
--            See services/verification-engine/src/infrastructure/postgres.rs
--            `run_dev_migrations_if_enabled` for the gating logic.
--
--            The shape and column types are intentionally inert (no
--            keys, no constraints beyond the PK already declared in
--            the production migration). The seed itself is harmless
--            data used by `MockBunecAdapter` to satisfy Stage 2
--            identity lookups during local development and the
--            integration-smoke suite.

BEGIN;

-- Seed: a handful of synthetic persons that match the canonical
-- declarant-portal seed data. UUIDs are deterministic v4 values
-- chosen so repeated dev startups land on the same rows (idempotent
-- via ON CONFLICT DO NOTHING).
INSERT INTO mock_bunec_persons (person_id, canonical_full_name, nationality)
VALUES
    ('11111111-1111-4111-8111-111111111111', 'Alpha Mock', 'CM'),
    ('22222222-2222-4222-8222-222222222222', 'Beta Mock', 'CM'),
    ('33333333-3333-4333-8333-333333333333', 'Gamma Mock', 'FR')
ON CONFLICT (person_id) DO NOTHING;

COMMIT;
