# ADR-0001: Event sourcing for the Declaration aggregate

**Status:** Accepted (since 2026-05-11)
**Decision-makers:** @recor/architect-team
**Date:** 2026-05-11 (commit `cbcc251`)

## Context

The Declaration service is the entry point of RÉCOR's declared-data
flow: it accepts beneficial-ownership declarations from the Declarant
Portal, validates the canonical domain invariants, verifies the
declarant's Ed25519 cryptographic attestation, and persists the
declaration so that downstream services (verification engine, public
consumers, regulators) can read it. The service was specified in
Architecture V4 P13 § Declaration Service.

Two facts about the domain dominate the design space:

1. **Regulatory audit obligations.** Beneficial-ownership registries
   are AML/CFT (anti-money-laundering / counter-financing-of-terrorism)
   infrastructure. The audit trail is not a feature — it is the
   regulatory product. A future investigator must be able to ask
   "what did the platform believe about entity X at instant T?" and
   get a defensible answer. Doctrine D15 ("cryptographic provenance")
   makes this binding: every consequential state transition must be
   provable after the fact.
2. **Mutability is the adversary.** The dominant adversarial pattern
   on a registry is not a one-shot lie at submission time — it is a
   sequence of "corrections" that drift the recorded structure over
   months. A CRUD model that overwrites prior rows loses precisely
   the evidence regulators need.

A third constraint is operational: at submission time we must also
trigger downstream verification, which means an outbox row must be
written transactionally with the state change. Whatever persistence
model we pick has to compose cleanly with the transactional outbox
pattern.

The two realistic options at the time of decision were:

- **CRUD-with-audit-table** — the typical Rails / Django shape: a
  current-state `declarations` table plus an `audit_log` side table.
  The audit table is informational; the system of record is the
  current row.
- **Event sourcing** — append-only `declaration_events` with a derived
  current-state projection. The event log is the system of record;
  the projection is a cached read model that can be dropped and
  rebuilt.

## Decision

We chose **event sourcing**. The system of record is
`declaration_events`; `declarations` is a projection of the event
stream. Implemented in commit `cbcc251` (the first real Declaration
service commit).

Specifics:

- **Events** live in `services/declaration/src/domain/event.rs`. The
  `DeclarationEvent` enum has three variants today —
  `Submitted(DeclarationSubmittedV1)`,
  `Verified(DeclarationVerifiedV1)`,
  `Superseded(DeclarationSupersededV1)` — and all payloads are
  versioned with a `V1` suffix so future schema changes produce new
  variants rather than breaking changes to existing ones.
- **The aggregate** at `services/declaration/src/domain/aggregate.rs`
  hydrates from `DeclarationEvent::from_events(id, &[events])` and
  applies events via a pure `apply()` function. The aggregate is
  pure Rust; no I/O. Commands (Submit, RecordVerificationOutcome,
  Supersede) return events; the use-case layer decides whether to
  apply and persist them.
- **The projection** `declarations` is upserted in the same Postgres
  transaction as the event insert and the outbox write. All three
  writes are atomic. Migration `0001_initial.sql` creates the
  schema; migration `0003_*` extends the projection with verification
  fields without touching the event payload.
- **Replay** is a first-class operation: dropping the projection and
  rebuilding it from `declaration_events` is the supported recovery
  procedure. The event ordering uses UUIDv7 to give a monotonic key
  without an autoincrement bottleneck.
- **Optimistic concurrency** is enforced via the aggregate's
  `version` field. A command that would emit an event against a
  version it didn't observe fails — no last-writer-wins.

## Consequences

### Positive

- The audit trail is the system of record, not a parallel structure
  that could diverge from it. There is no "but what did the audit
  log say?" question — the audit log *is* the data.
- Verification outcomes (`DeclarationVerifiedV1`), supersessions
  (`DeclarationSupersededV1`), and future amendments (`R-DECL-3`)
  are additive: a new event variant + an aggregate match arm + a
  projection update. The schema does not need to be re-shaped for
  each new domain operation.
- The transactional outbox pattern composes cleanly. The outbox
  insert sits next to the event insert and the projection upsert,
  inside one Postgres transaction. Every committed event has an
  outbox row; every uncommitted event has neither.
- Idempotency is structural. The aggregate's `version` field
  prevents accidental double-emit; the Idempotency-Key header
  prevents accidental double-submit at the API boundary.
- Replay-based recovery: if the projection ever becomes corrupt
  (e.g. a schema bug), it can be truncated and rebuilt from the
  event log. The event log itself never needs surgery.

### Negative

- Two-table writes per command. Every state change inserts to
  `declaration_events`, upserts `declarations`, and inserts to
  `outbox`. The transaction is small but it is not free.
- Cognitive load on the next engineer. Engineers familiar with
  Active-Record CRUD will reach for "update the row" reflexively;
  this codebase forces them through "emit event → apply → save".
  The `CLAUDE.md` for the service is explicit about this so the
  onboarding cost is paid once.
- Event-schema evolution is forever. `DeclarationSubmittedV1` is
  immutable in shipped form. A version 2 means adding a new variant
  and teaching the aggregate's `apply()` to handle both — old
  events never get migrated in place.

### Neutral

- We are not running full CQRS. The projection is a simple Postgres
  upsert in the same transaction as the event write, not a separate
  read-model service consuming an event stream. We may evolve toward
  separate read models if reporting requirements grow; today the
  simple projection is sufficient.
- We do not yet have event snapshots. Aggregates rehydrate by
  replaying every event for the id. Declarations rarely accumulate
  many events (Submit → Verify → optionally Supersede), so the
  cost is bounded. If amendments make per-aggregate event counts
  grow, snapshot support is an additive change.

## Alternatives considered

### CRUD-with-audit-table

Rejected. The current-state row is the system of record; the audit
table is a derivative that the application is responsible for keeping
honest. Every code path that updates a row must remember to write the
audit row, and a bug in that discipline silently destroys the audit
trail. For a registry whose audit trail *is* the regulatory product,
"the application has to remember" is the wrong primitive.

### Event sourcing with full CQRS (separate read model)

Rejected for v1. CQRS adds an asynchronous projection lag and a
separate read-side store. Our reads are simple ("look up this
declaration", "list this declarant's declarations") and tolerate
strong consistency from a Postgres projection. We retain the option
to add a denormalised read store later as a downstream consumer of
the existing outbox.

### Hyperledger Fabric as the system of record

Rejected for v1; deferred to `R-DECL-9`. Anchoring receipts to a
Fabric audit channel is on the roadmap to give external verifiers
a non-RÉCOR-trusted attestation. But Fabric is not a transactional
database, and using it as the primary store would make the hot
write path depend on chaincode latency and a peer quorum. The chosen
shape (Postgres event log + Fabric anchor of receipts) keeps the
write path local and adds Fabric as an externalisation step.

## References

- Commit `cbcc251` — initial Declaration service (`feat(declaration):
  the first real platform service — accept + verify + persist + serve`)
- `services/declaration/CLAUDE.md`
- `services/declaration/src/domain/aggregate.rs` (top-of-file doc)
- `services/declaration/src/domain/event.rs`
- `services/declaration/migrations/0001_initial.sql`
- Architecture V4 P13 § Declaration Service
- Architecture V4 P14 § Canonical Data Model
- Doctrines D13 (idempotency), D14 (fail-closed), D15 (cryptographic
  provenance)
- Follow-up: `R-DECL-3` (amend / correction / supersede commands —
  partially closed by PR #55), `R-DECL-9` (Fabric anchoring)
