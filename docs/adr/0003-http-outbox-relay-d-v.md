# ADR-0003: HTTP outbox-relay for the D↔V loop (interim before Kafka)

**Status:** Accepted (since 2026-05-11); superseded by Kafka transport
when `R-LOOP-2` lands
**Decision-makers:** @recor/architect-team
**Date:** 2026-05-11 (commit `bb51443`, PR #38); 2026-05-11 (commit
`8b7f262`, PR #39)

## Context

The Declaration service (`recor-declaration`) and the Verification
Engine (`recor-verification-engine`) must talk to each other in two
directions:

- **D → V**: a freshly-submitted declaration must reach the
  verification engine so the 9-stage pipeline can run.
- **V → D**: the verification engine's lane decision must reach the
  declaration so the aggregate transitions to Accepted, InVerification,
  or Rejected. The Declarant Portal polls the declaration to surface
  this state to the declarant.

The target architecture (Architecture V4 P14 + the Companion's
event-bus section) calls for Kafka topics
`recor.declaration.events.v1` and `recor.verification.events.v1` as
the transport. Kafka gives durable, partitioned, at-least-once
delivery with consumer groups, replay from offsets, and well-known
operational patterns.

At the time of decision (May 2026, ~3 PRs after the first commits
of each service) no Kafka cluster existed. Provisioning Kafka — even
a single-node KRaft instance — would have been a multi-week side
quest before the D↔V loop could close end-to-end. The roadmap tracks
this work as `R-LOOP-2` (#35, ~2 weeks once a cluster is available).

The pragmatic constraint: the D↔V loop is the architectural backbone
of the platform. Until it closes, declarations submitted to D do not
auto-flow into verification, and verification outcomes do not
auto-flow back. Manual intervention is not a v1 option (Doctrine D14,
fail-closed at integration boundaries — and the manual-intervention
state is silently fail-open). We needed a transport we could ship
*now* that preserved the semantics of the eventual Kafka transport
well enough that the swap would not be observable to the application.

## Decision

We chose an **HTTP outbox-relay** as the v1 transport. The semantics
that matter — at-least-once delivery, authenticated sender, retry
with backoff, exactly-once-effective consumer via idempotency keys —
are all preserved. The transport itself is the only thing that
changes when `R-LOOP-2` lands. Implemented in PR #38 (Phase 1, D→V,
commit `bb51443`) and PR #39 (Phase 2, V→D, commit `8b7f262`).

Specifics:

- **Outbox tables.** Each service writes a row to its `outbox`
  (declaration) or `verification_outbox` (verification engine) in
  the same Postgres transaction as the state change. The outbox row
  is the durable representation of the cross-service event; if the
  transaction commits, the event exists.
- **Background relay task.**
  `services/declaration/src/infrastructure/relay.rs` and the
  matching V-engine module poll their outbox every 5s (configurable
  via `RELAY_POLL_INTERVAL_SECONDS`), envelope each undispatched
  row, sign it, POST to the subscriber's webhook URL, and mark
  `dispatched_at` on a 2xx response. Failures increment
  `dispatch_attempts` and record `last_error`. The relay shuts down
  on a `CancellationToken` from `main.rs` so the service exits
  cleanly.
- **HMAC-SHA256 over the raw body.** The relay signs the POST body
  with a shared secret and includes the signature in the
  `X-RECOR-Signature` header. The verifier re-computes the HMAC on
  the body it received and rejects on mismatch with a constant-time
  compare. See ADR-0005 for the per-channel-secrets + dual-secret
  rotation design that extended this primitive.
- **Idempotency at the consumer.** The verification engine's
  `POST /v1/internal/declaration-events` and the declaration
  service's `POST /v1/internal/verification-outcomes` both treat
  replays of the same event as no-ops at the database level. The
  aggregate's optimistic concurrency check prevents double-applied
  events; the use-case layer returns 200 on idempotent replay (vs
  201 on first write), so the relay learns to stop retrying.
- **Dead-letter queue.** `R-LOOP-4-DLQ` (PR #57) added the
  `outbox_dlq` / `verification_outbox_dlq` tables: rows that exceed
  `max_attempts = 12` move into the DLQ where they no longer pull
  on the dispatcher. Admin endpoints (PRs #59, #63) let an operator
  list and replay DLQ rows after the underlying issue is fixed.
- **Bounded delivery time.** With `poll_interval = 5s` and
  `max_attempts = 12`, the relay drains within ~60s in the happy
  path; the rotation runbook adds 30s safety for a total
  90-second drain window.

The contract between services is explicit. The D→V envelope carries
the `declaration.submitted.v1` event shape; the V→D envelope is a
slim `verification.completed.v1` (case_id, declaration_id, lane,
fused authenticity belief + plausibility, fused risk belief,
completed_at) — *not* the full case payload. The cross-service
contract test
`services/declaration/tests/writeback_contract.rs` locks the shape;
field renames on either side fail this test in CI rather than at
integration-smoke time.

## Migration plan (the most important part of this ADR)

When `R-LOOP-2` lands:

1. **Producer change.** The relay task is replaced with a Kafka
   producer. The outbox row is still written transactionally with
   the state change; an outbox-relay-style consumer publishes the
   row to the corresponding topic and marks `dispatched_at`. The
   outbox tables persist — they are not Kafka-specific.
2. **Consumer change.** The HTTP `/v1/internal/declaration-events`
   and `/v1/internal/verification-outcomes` endpoints stay (they
   are useful for replay, manual injection, and smoke tests). The
   primary consumer becomes a Kafka consumer that calls the same
   use-case as the HTTP handler.
3. **Idempotency holds.** The consumer-side idempotency was
   designed for at-least-once and survives the transport swap
   unchanged.
4. **HMAC retires.** Kafka's mTLS + topic ACLs replace the
   HMAC-on-the-body primitive. The per-channel secrets become a
   pre-Kafka curiosity rather than an ongoing operational concern.
5. **Auth supersession.** `R-LOOP-3` separately migrates the
   service-to-service auth to SPIFFE+mTLS via SPIRE. The Kafka
   migration and the auth migration can land in either order.

## Consequences

### Positive

- The D↔V loop closes today. The portal can poll a declaration and
  see its verification state transition without any manual step.
- Operational primitives we wanted anyway — outbox tables, DLQ,
  admin endpoints, the dispatch_attempts counter — are in place
  and exercised. They survive the Kafka swap unchanged.
- HMAC-over-body authentication is well-understood and easy to
  operate. The rotation runbook (see ADR-0005) makes secret
  rotation a non-event.
- The cross-service contract is locked by a real test
  (`writeback_contract.rs`). Drift surfaces in `cargo test --lib`,
  not in production.

### Negative

- HTTP webhooks are point-to-point. A future third consumer of
  declaration events (e.g. a search-indexer) requires either
  multiple subscribers in the relay or a real broker. We are not
  building toward many consumers on this transport; we are
  building toward swapping it.
- The 5-second poll interval is the lower bound on D→V latency. A
  Kafka producer/consumer pair would push that to <100ms. For v1,
  5-second latency is acceptable — humans are the consumers.
- Replay-from-offset does not exist. To re-process an event the
  operator must manually copy the outbox row back to undispatched
  state. With Kafka, "rewind the consumer to offset X" is one
  command.
- Two secrets to operate (one per channel; see ADR-0005). Kafka
  with mTLS retires these.

### Neutral

- The Phase 1 (#38) and Phase 2 (#39) commits each ship their own
  copy of the relay code (one in `services/declaration`, one in
  `services/verification-engine`). Consolidation into a shared
  crate is feasible but low-value — the code is small and the
  swap to Kafka will rewrite both anyway.
- `relay.rs` carries a `R-DECL-2` reference noting the Kafka
  migration; the file is intended to be deleted, not maintained
  long-term.

## Alternatives considered

### Direct service-to-service POST (no outbox)

Rejected. Without an outbox, a successful state-change commit
followed by a failed HTTP POST silently drops the cross-service
event. The next commit might succeed; the gap is invisible. At-least-
once delivery requires durably enqueuing the intent to publish.

### Kafka from day 1

Rejected for the interim. Provisioning Kafka before the D↔V loop
existed would have delayed the loop's first close by weeks. With
the outbox-relay shape, we close the loop in PRs #38 and #39 and
swap the transport later, when a cluster exists. The outbox tables
*are* the Kafka-readiness work.

### Managed event bus (AWS EventBridge, GCP Pub/Sub)

Rejected for the long term. Beneficial-ownership registries are
sovereign infrastructure; the consortium retains the option to
move clouds (see ADR-0001's observability ADR for the parallel
reasoning on Grafana Cloud). A vendor-managed bus locks the
transport to a cloud. Open-source Kafka avoids the lock-in.

### NATS / RabbitMQ / etc.

Considered as alternatives to Kafka. The architecture commits
specifically to Kafka semantics (durable log, partitioning, offsets,
consumer groups). Substituting a different broker is feasible but
not a savings — the migration work is the same shape. We stick to
the architectural commitment.

## References

- PR #38 (commit `bb51443`) — D→V Phase 1
- PR #39 (commit `8b7f262`) — V→D Phase 2
- PR #57 (commit `a7586e6`) — R-LOOP-4-DLQ dead-letter queue
- PR #59 (commit `4d0b49e`) — R-LOOP-DLQ-2 admin endpoints
  (declaration side); PR #63 mirrors them onto the V-engine
- `services/declaration/src/infrastructure/relay.rs` (top-of-file
  doc explicitly flags the Kafka migration)
- `services/declaration/tests/writeback_contract.rs` —
  cross-service envelope contract test
- `docs/runbooks/hmac-secret-rotation.md` — operates the channels
- `docs/ROADMAP.md` Track L — R-LOOP-2 (Kafka, ~2 weeks),
  R-LOOP-3 (SPIFFE+mTLS, ~2 weeks)
- Architecture V4 P14 § event bus
- Doctrines D14 (fail-closed), D17 (zero trust at every network
  boundary)
