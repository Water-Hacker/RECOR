-- Migration: 0004_add_kafka_consumer_dlq
-- Service:   verification-engine
-- Sprint:    PI-2 (R-LOOP-2 — Kafka transport)
-- Rationale: the Kafka consumer parses incoming messages from
--            `recor.declaration.events.v1` and feeds them through
--            `SubmitVerificationUseCase`. Two failure modes need
--            durable forensics:
--              1. The message bytes are not parseable as the expected
--                 envelope (schema regression on the producer side,
--                 malformed payload, etc.). Retrying does not help —
--                 the consumer commits the offset and writes the raw
--                 bytes to this DLQ so a human can investigate.
--              2. The use case returns an error after the bounded
--                 retry budget is exhausted (e.g. persistent DB
--                 outage in the V-engine itself, or a domain
--                 invariant the producer should have caught). The
--                 consumer writes a structured row to this DLQ and
--                 commits the offset (D14: fail-closed at the
--                 integration boundary — we never block the topic
--                 on a single poisoned message).
--
-- Shape mirrors `verification_outbox_dlq` (the outbound side) so a
-- single DLQ-admin code path can list / replay from either side once
-- the consumer admin endpoints land in a follow-up. The `topic` and
-- `partition` columns are Kafka-specific (no analogue in the HTTP
-- DLQ) — they're the precise replay coordinates the operator needs.
--
-- Idempotency contract: the `event_id` column is UNIQUE so a
-- double-write (e.g. two consumer instances racing on the same
-- failed message before offsets commit) collapses to one DLQ row
-- rather than creating duplicates. The use-case-error path uses
-- INSERT ... ON CONFLICT DO NOTHING.

BEGIN;

CREATE TABLE IF NOT EXISTS kafka_consumer_dlq (
    -- Primary identity. Generated client-side as a v7 UUID so the
    -- index is roughly chronological and DLQ admin scans are cheap.
    id                  UUID PRIMARY KEY,

    -- Idempotency anchor. NULL only for parse-failure rows where we
    -- could not extract an event_id from the message bytes. The
    -- partial unique index below covers the common case.
    event_id            UUID UNIQUE,

    -- Kafka coordinates: topic + partition + offset together uniquely
    -- identify the message on the broker, which is what an operator
    -- needs to inspect / replay manually with kafka-console-consumer.
    topic               TEXT NOT NULL,
    partition           INTEGER NOT NULL,
    "offset"            BIGINT NOT NULL,

    -- Raw envelope as received from Kafka. JSONB when parse succeeded;
    -- NULL otherwise (raw bytes captured in `raw_payload` instead).
    -- D18 (no secrets): outbox payloads are domain events, never
    -- credentials; storing them here is fine.
    payload             JSONB,

    -- Raw message bytes (BYTEA) — populated for parse-failure rows so
    -- a human can inspect the malformed bytes. NULL when payload
    -- parsed successfully.
    raw_payload         BYTEA,

    -- Why the row landed here:
    --   `parse_error`         — envelope or payload could not be deserialised
    --   `usecase_error`       — submit-verification returned a domain error
    --   `retry_exhausted`     — bounded retry budget hit for usecase_error
    failure_kind        TEXT NOT NULL,

    -- Human-readable error string. Bounded (TEXT — Postgres caps it
    -- at 1 GB but the consumer truncates to 8 KB).
    last_error          TEXT NOT NULL,

    -- How many in-process retries we tried before dead-lettering.
    -- Zero for parse_error (no retry; parse failures are permanent).
    retry_attempts      INTEGER NOT NULL DEFAULT 0,

    -- When the message was originally consumed (broker-stamped, not
    -- the time we wrote this row).
    consumed_at         TIMESTAMPTZ NOT NULL,

    -- When this DLQ row was written.
    dead_lettered_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Useful indexes for DLQ admin scans (latest-first, filter by topic).
CREATE INDEX IF NOT EXISTS idx_kafka_consumer_dlq_dead_lettered_at
    ON kafka_consumer_dlq (dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_kafka_consumer_dlq_topic
    ON kafka_consumer_dlq (topic, dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_kafka_consumer_dlq_failure_kind
    ON kafka_consumer_dlq (failure_kind, dead_lettered_at DESC);

COMMIT;
