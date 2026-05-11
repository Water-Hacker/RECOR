-- Migration: 0001_initial
-- Service:   verification-engine
-- Sprint:    PI-1 (Verification engine v1)
-- Rationale: persist verification cases and a mock BUNEC record set.

BEGIN;

CREATE TABLE IF NOT EXISTS verification_cases (
    case_id             UUID PRIMARY KEY,
    declaration_id      UUID NOT NULL UNIQUE,
    entity_id           UUID NOT NULL,
    declarant_principal TEXT NOT NULL,
    lane                TEXT NOT NULL CHECK (lane IN ('green', 'yellow', 'red')),
    authenticity_belief DOUBLE PRECISION NOT NULL,
    authenticity_plausibility DOUBLE PRECISION NOT NULL,
    risk_belief         DOUBLE PRECISION NOT NULL,
    case_payload        JSONB NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    completed_at        TIMESTAMPTZ NOT NULL,
    total_duration_ms   BIGINT NOT NULL CHECK (total_duration_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_verification_cases_declaration
    ON verification_cases(declaration_id);
CREATE INDEX IF NOT EXISTS idx_verification_cases_entity
    ON verification_cases(entity_id);
CREATE INDEX IF NOT EXISTS idx_verification_cases_lane
    ON verification_cases(lane)
    WHERE lane IN ('yellow', 'red');

-- Mock BUNEC record store for dev/test. Real BUNEC integration is a
-- separate ticket; this table is what the MockBunecAdapter consults.
CREATE TABLE IF NOT EXISTS mock_bunec_persons (
    person_id           UUID PRIMARY KEY,
    canonical_full_name TEXT NOT NULL,
    nationality         TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Outbox for future verification-outcome events.
CREATE TABLE IF NOT EXISTS verification_outbox (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    event_version       INTEGER NOT NULL CHECK (event_version >= 1),
    aggregate_id        UUID NOT NULL,
    partition_key       TEXT NOT NULL,
    payload             JSONB NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatched_at       TIMESTAMPTZ,
    dispatch_attempts   INT NOT NULL DEFAULT 0,
    last_error          TEXT
);

CREATE INDEX IF NOT EXISTS idx_verification_outbox_undispatched
    ON verification_outbox(created_at)
    WHERE dispatched_at IS NULL;

COMMIT;
