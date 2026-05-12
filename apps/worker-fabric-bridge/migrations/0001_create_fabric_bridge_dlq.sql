-- Migration: 0001_create_fabric_bridge_dlq
-- Service:   worker-fabric-bridge
-- Sprint:    R-DECL-9
-- Rationale: permanent failures from the Fabric Bridge land here for
--            forensic review and manual re-anchor. The shape mirrors
--            services/declaration/migrations/0005_add_outbox_dlq.sql so
--            operators can re-use existing forensics tooling.
--
-- Atomicity: rows are inserted by the worker after FabricBridge::commit_audit_entry
-- exhausts its retry budget OR returns a non-retryable error. The
-- worker writes a single INSERT per failure; ON CONFLICT (event_id) DO
-- NOTHING preserves idempotency if the relay drives a second failure.
--
-- Retention: NEVER pruned. The DLQ is the forensic record of
-- declaration events that did not anchor; clearing it requires
-- explicit operator action via the runbook (docs/runbooks/fabric-bridge.md).
--
-- Properties verified post-migration:
--   1. fabric_bridge_dlq.event_id is UNIQUE
--   2. cause IN ('permanent', 'non_retryable', 'config') is enforced

BEGIN;

CREATE TABLE IF NOT EXISTS fabric_bridge_dlq (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id            UUID NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    aggregate_id        UUID NOT NULL,
    payload             JSONB NOT NULL,
    attempts            INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error          TEXT NOT NULL,
    cause               TEXT NOT NULL CHECK (cause IN ('permanent', 'non_retryable', 'config')),
    dead_lettered_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Forensics query patterns:
--   1. "Show me everything dead-lettered in the last 24h"
--   2. "Show me failures for a particular declaration aggregate"
--   3. "Group by cause to see whether the gateway is misbehaving or the
--       chaincode is rejecting input"
CREATE INDEX IF NOT EXISTS idx_fabric_bridge_dlq_dead_lettered_at
    ON fabric_bridge_dlq (dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_fabric_bridge_dlq_aggregate
    ON fabric_bridge_dlq (aggregate_id);

CREATE INDEX IF NOT EXISTS idx_fabric_bridge_dlq_cause
    ON fabric_bridge_dlq (cause);

COMMIT;
