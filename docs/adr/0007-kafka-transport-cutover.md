# ADR-0007: Kafka transport cutover plan for the D↔V loop

**Status:** Proposed (skeleton ships in `feat/loop-2-kafka`); becomes
Accepted when the dual-transport smoke passes in CI and the operator
runbook is ready
**Decision-makers:** @recor/architect-team, @recor/sre-team
**Date:** 2026-05-12

## Context

ADR-0003 ("HTTP outbox-relay for the D↔V loop") committed RÉCOR to a
Kafka-based transport once a cluster existed, and tracked the migration
work as `R-LOOP-2`. That ticket now ships:

- Single-broker dev Kafka under `infrastructure/kafka/` (KRaft mode).
- `KafkaProducer` in `services/declaration/src/infrastructure/kafka_producer.rs`,
  reading the existing outbox table and publishing each row keyed by
  `aggregate_id` to `recor.declaration.events.v1`.
- `KafkaConsumer` in `services/verification-engine/src/infrastructure/kafka_consumer.rs`,
  reading the same topic and feeding `SubmitVerificationUseCase` —
  the same use case the HTTP webhook calls.
- Config switches: `RELAY_TRANSPORT=http|kafka` on the declaration
  service, `VERIFICATION_TRANSPORT=http|kafka` on the V-engine. Both
  default to `http` (no behavioural change for existing deployments).
- A `kafka-consumer_dlq` table mirroring the outbox DLQ shape for
  parse-failure and retry-exhausted forensics.
- Prometheus metrics: `recor_kafka_produce_total`,
  `recor_kafka_produce_latency_seconds` (declaration);
  `recor_kafka_consume_total`, `recor_kafka_consume_lag_seconds`
  (verification-engine).

The decision before us is the *cutover*. The skeleton is in; how do we
go from "skeleton + HTTP" to "Kafka only", safely, in production?

## Decision

We adopt a **four-phase staggered cutover** with a feature flag at
every step. The flag values are the env switches above; the operator
flips them per-environment in this order:

### Phase 0 — dev parity (this PR)

- Both transports build cleanly.
- `RELAY_TRANSPORT=http` and `VERIFICATION_TRANSPORT=http` everywhere
  (the defaults). Existing tests + smoke pass unchanged.
- The kafka-smoke (`services/declaration/scripts/kafka-smoke.sh`)
  brings up the dual-transport stack on developer laptops and proves
  the round-trip works.

### Phase 1 — staging dual-transport (week 1 after merge)

- Staging gets `RELAY_TRANSPORT=kafka` AND keeps `RELAY_WEBHOOK_URL`
  set. The declaration emits each event TWICE — once via HTTP, once
  via Kafka. Likewise the V-engine has both `VERIFICATION_TRANSPORT=kafka`
  AND its HTTP `/v1/internal/declaration-events` handler active.
- The V-engine's idempotency-on-event-id absorbs the duplicate: the
  first delivery (whichever arrives first) creates the case; the
  second delivery returns the existing case without re-applying state.
  This is the same property that lets the HTTP relay's at-least-once
  retries work today, so dual-transport is just a degenerate case
  of the same shape.
- Operators monitor `recor_kafka_consume_total{result=...}` and the
  HTTP webhook's 200/5xx ratio to ensure parity (every event the HTTP
  path applied, the Kafka path also applied or skipped as a
  duplicate). The dashboards for OBS-1 (Architecture V5 P22) carry
  the relevant panels.

### Phase 2 — production dual-transport (week 2)

- Production gets the same dual-transport configuration. Operators
  re-run the parity check there.
- If parity holds for one week with no DLQ accumulation, proceed to
  Phase 3. If parity diverges, flip back to `RELAY_TRANSPORT=http`
  (single env flip; no rollback of code).

### Phase 3 — HTTP retired (week 4+)

- Set `RELAY_TRANSPORT=kafka` AND `RELAY_WEBHOOK_URL=""` on the
  declaration. The HTTP outbox-relay stops spawning. Same for the
  V-engine: clear `INBOUND_HMAC_SECRET` so the HTTP webhook returns
  503.
- After one further week of clean operation, delete the HTTP relay
  code in a separate PR (`R-LOOP-2-cleanup`). The HMAC primitive and
  rotation runbook retire alongside it; service-to-service auth
  becomes the Kafka SASL + topic ACLs path (and `R-LOOP-3` for
  SPIFFE+mTLS).

### What does NOT change in this ADR

- The wire shape — payload stays JSON, byte-for-byte identical to the
  HTTP relay envelope. A schema-registry migration (Avro or Protobuf
  with Confluent Schema Registry) is tracked as a separate follow-up
  (`R-LOOP-5-schema`). The v1 topic suffix is the schema version;
  v2 lands when the schema changes.
- The outbox table — both transports read it. It remains the durable
  representation of the cross-service event; if the Postgres
  transaction commits, the event is published exactly when the
  producer drains the row. The outbox tables outlive the HTTP relay.
- The DLQ semantics — `outbox_dlq` still handles producer-side
  failures; the new `kafka_consumer_dlq` is for consumer-side failures
  (parse error, retry-exhausted). The two DLQs are forensic surfaces;
  the admin UI consolidates them in a follow-up.

## Consequences

### Positive

- The cutover is reversible at every step. Each phase is a single env
  flip, not a code deploy.
- Dual-transport during weeks 1-3 catches consumer-side regressions
  before they're load-bearing. We never have a window where Kafka is
  the only delivery path *and* hasn't proven parity with HTTP.
- The same outbox + idempotency primitives the HTTP path exercises
  today carry the Kafka path. No new failure modes are introduced
  by the transport swap; the well-understood ones are the only ones.
- The schema-registry migration is decoupled. We don't block Kafka
  rollout on Avro/Protobuf adoption.

### Negative

- During dual-transport (weeks 1-3) every event lands twice. The
  V-engine's submit_verification path sees double the load (though
  one of every pair is a fast idempotency replay). Dashboard
  capacity headroom is the constraint.
- Operators carry two transport switches per service plus the HMAC
  rotation primitives for the duration of Phase 1-3. Cognitive load
  is real; the operator runbook (`docs/runbooks/kafka-cutover.md`,
  follow-up) documents the day-2 procedures.
- The DLQ shape diverges between the two transports until they're
  consolidated. Operators need to know to check both DLQ tables
  during incidents. The admin UI consolidation is a separate ticket.

### Neutral

- The kafka-consumer_dlq schema includes `topic`, `partition`, and
  `offset` columns that have no analogue in the outbox DLQ. These
  are the precise replay coordinates an operator needs; they're
  worth the schema divergence.
- `enable.idempotence=true` on the producer eliminates broker-side
  duplicates from retries. The V-engine's existing event_id-based
  idempotency still catches consumer-side replays (Kafka's
  at-least-once on the consume side is unchanged by the producer
  setting).

## Alternatives considered

### Big-bang cutover (no dual-transport phase)

Rejected. Flipping from HTTP to Kafka in one step puts the entire
declaration→verification flow on an untested production transport.
A latent bug in the consumer (e.g. a malformed message that the unit
tests didn't catch) would cause cases to silently stop appearing —
the exact failure mode ADR-0003 was designed to avoid for HTTP.

### Replace HTTP with Kafka, no HTTP fallback

Rejected for v1. The HTTP relay is mature and well-instrumented; it
costs little to keep alive during the cutover window. Removing it
before Kafka is proven would be one of those "well-meaning" cleanups
that compound into a production-impacting decision.

### Migrate to a schema registry first, then to Kafka

Rejected as scope expansion. The transport swap and the schema
migration are independent decisions; coupling them doubles the
review surface for no operational benefit. The v1 topic suffix
ensures we can introduce a schema-registry-aware v2 topic without
disrupting v1 consumers.

### Use Kafka transactions (idempotent producer + transactional commit)

Considered. The transactional producer would let us coordinate
`producer.commit_transaction()` with the outbox `UPDATE ... SET
dispatched_at = NOW()`. That's stronger than what we ship — the v1
ordering is "ack from broker → mark dispatched", which has a small
window where the broker has the message but the outbox row is still
flagged undispatched. A crash in that window causes a redelivery
on next poll, which the consumer's idempotency absorbs. Adding
transactional producer support is a follow-up (`R-LOOP-2-txn`); v1
ships the simpler shape.

## References

- ADR-0003 — the HTTP relay this ADR supersedes
- `docs/PRODUCTION-TODO.md` § R-LOOP-2 — the ticket scope
- `infrastructure/kafka/README.md` — dev Kafka cluster usage
- `services/declaration/src/infrastructure/kafka_producer.rs`
- `services/verification-engine/src/infrastructure/kafka_consumer.rs`
- `services/verification-engine/migrations/0004_add_kafka_consumer_dlq.sql`
- `services/declaration/scripts/kafka-smoke.sh` — dual-transport smoke
- Architecture V4 P14 § Event bus — Kafka as the target transport
- Doctrines D13 (idempotency), D14 (fail-closed), D16 (observability)
