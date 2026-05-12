# Data retention policy — RÉCOR platform

Status: **In force from Phase 0 of platform operations** (COMP-2 shipped).
Author: RÉCOR security-engineering.
Reviewers: architect-reviewer, security-reviewer.
Last review: 2026-05.

This document defines how long each table in the RÉCOR platform's
operational databases is retained, the mechanism that enforces the
retention rule, and the legal / doctrinal basis for the choice.

The platform processes Cameroonian beneficial-ownership data. Two
external regimes inform retention floors:

- **OHADA AML/CFT Acte uniforme (2014)** — beneficial-ownership
  registries must retain declaration records for the lifetime of the
  registered entity plus a post-dissolution audit window.
- **FATF Recommendation 24** — beneficial-ownership information must
  remain available for verification and law-enforcement use.

Both regimes treat the *audit trail* of who declared what and when as
the load-bearing record, not the operational queues that move that
data between services. The policy below distinguishes the two: audit
records are retained forever; operational queues are pruned on a
documented cadence.

## Governing doctrines

- **D15 cryptographic provenance** — every consequential event has a
  durable, tamper-evident record. Pruning the event log would
  invalidate every BLAKE3 receipt that pins to it.
- **D14 fail-closed** — the retention worker's safe default
  (`OUTBOX_RETENTION_DAYS=0`) is "do not prune". Pruning is opt-in,
  per environment.
- **D16 observability** — every prune cycle emits a metric +
  structured log; alerts can detect a worker that has silently
  stopped (counter rate drops to zero unexpectedly).
- **D17 zero trust** — immutability is enforced at the SQL boundary
  by trigger, not by trusting application code to behave.

## Threat-model linkage

This policy partially closes **Gap G1** from
`docs/security/threat-model.md` (no in-DB audit chain on the event
log). Full closure requires the higher-layer audit anchor that
**R-DECL-9** introduces (Hyperledger Fabric audit channel). Until
R-DECL-9 ships, the trigger-and-grant model documented here is the
sole mechanism enforcing event-log immutability.

## Per-table policy

The two services own different tables, but the retention shape is
the same: append-only audit tables retained forever, operational
queues pruned after a documented window, DLQs retained forever for
forensic use, idempotency caches auto-expire by row.

### Declaration service

| Table | Retention | Mechanism | Notes |
|---|---|---|---|
| `declaration_events` | **Forever** | Append-only — UPDATE/DELETE/TRUNCATE refused by trigger; UPDATE/DELETE/TRUNCATE revoked from PUBLIC; migration `0007_audit_log_immutability.sql` | The platform's source of truth for every state change. Receipts (D15) pin to rows in this table. Pruning would invalidate the entire receipt chain. |
| `declarations` | **Forever** | Projection of `declaration_events`; rebuildable from the event log | Current-state mirror. Kept indefinitely for read performance. UPDATE is permitted (the projection is mutable by design). |
| `outbox` | **30 days after `dispatched_at`** | Retention worker (`services/declaration/src/infrastructure/retention.rs`); TRUNCATE revoked from PUBLIC | Operational queue. Un-dispatched rows are NEVER pruned regardless of age (`dispatched_at IS NULL` rows survive). |
| `outbox_dlq` | **Forever** | UPDATE / TRUNCATE revoked from PUBLIC; the retention worker NEVER touches this table; INSERT + SELECT + DELETE retained for replay | Incident-investigation surface. Operators inspect, replay, or accept dead-lettered events; the original envelope bytes are preserved to support disputed-event review (D15). |
| `idempotency_records` | **Auto-expire at `expires_at`** (default 24 h) | Application checks `expires_at > NOW()` on every read; row-level TTL is enforced by query predicate, not a job | Idempotency tokens are short-lived by design. The `IDEMPOTENCY_TTL_SECONDS` env controls the window; defaults to 86400 (24 h). |

### Verification engine

| Table | Retention | Mechanism | Notes |
|---|---|---|---|
| `verification_cases` | **Forever** | Append-only — UPDATE/DELETE/TRUNCATE refused by trigger; UPDATE/DELETE/TRUNCATE revoked from PUBLIC; migration `0003_audit_log_immutability.sql` | Adjudication record. ADR-002 (Dempster-Shafer fusion math is auditable) requires inputs + BPAs to remain byte-identical post-adjudication. |
| `verification_outbox` | **30 days after `dispatched_at`** | Retention worker (`services/verification-engine/src/infrastructure/retention.rs`); TRUNCATE revoked from PUBLIC | Mirror of the declaration outbox policy. |
| `verification_outbox_dlq` | **Forever** | UPDATE / TRUNCATE revoked from PUBLIC; the retention worker NEVER touches this table | Mirror of the declaration DLQ policy. |
| `mock_bunec_persons` | **Dev/test fixture only** | No retention rule; replaced by the real BUNEC adapter under R-VER-1 | Out of scope for COMP-2 because it carries no production data. |

## Enforcement mechanisms

### 1. SQL-level immutability triggers (event logs)

`declaration_events` and `verification_cases` carry BEFORE UPDATE /
DELETE / TRUNCATE triggers that `RAISE EXCEPTION` with SQLSTATE
`insufficient_privilege`. The triggers fire regardless of the
invoking role, including the table owner — they are the load-bearing
guarantee. The `REVOKE` statements that accompany them strip
UPDATE / DELETE / TRUNCATE from `PUBLIC` so any future non-owner
"app" role inherits the same deny-by-default posture.

Coverage proof: `services/declaration/tests/audit_immutability.rs`
(testcontainers-gated) brings up Postgres 17, applies the migration
chain, and asserts that direct UPDATE / DELETE / TRUNCATE on
`declaration_events` returns an error containing the trigger's
exception message.

### 2. Retention workers (operational queues)

Each service runs a tokio task spawned at startup that, on a
configurable interval, executes:

```sql
DELETE FROM <outbox-table>
WHERE dispatched_at IS NOT NULL
  AND dispatched_at < NOW() - INTERVAL '<retention_days> days'
```

The worker:

- NEVER touches the event log or the DLQ;
- NEVER deletes a row where `dispatched_at IS NULL` (un-relayed events
  remain regardless of age);
- emits the `recor_outbox_retention_pruned_total{result}` metric on
  every cycle so operators can alert on "rate dropped to zero" (worker
  stopped) or "rate spiked" (something fed the outbox excess rows).

Environment variables (same names in both services):

| Env var | Default | Effect |
|---|---|---|
| `OUTBOX_RETENTION_DAYS` | `0` | `0` DISABLES pruning. Production should set this to `30` (the architectural commitment). |
| `OUTBOX_RETENTION_INTERVAL_SECONDS` | `86400` (daily) | Cycle interval. Values below 60 emit a startup warning (likely a unit confusion). |

### 3. Idempotency TTL (Declaration only)

`idempotency_records.expires_at` is enforced by the application's
query predicate (`expires_at > NOW()` on every lookup). Stale rows
are tolerated (they're filtered out by the predicate); a future
ticket may add a background cleanup, but the correctness of replay
detection does not depend on it.

## How to change a retention rule

1. Open an ADR documenting the change and the legal review.
2. Update the table above and the worker / migration / TTL that
   enforces the rule.
3. Run `cargo test --workspace --lib` + the
   `audit_immutability.rs` integration tests against the change.
4. Add a release-notes entry — operators rely on this document for
   audit-trail expectations.

Changes to the event-log immutability rule (`declaration_events`,
`verification_cases`) require additional review from
`security-reviewer` and `architect-reviewer` and are NOT permitted
without an explicit doctrine waiver (D15).

## Operational runbook hooks

- `docs/runbooks/restore-database-from-backup.md` — full restore
  procedure; the retention windows here define the maximum data loss
  a single restore can recover from.
- `docs/runbooks/observability-dashboards.md` — Grafana panel for
  `recor_outbox_retention_pruned_total`; alert on rate drops to zero
  for more than two cycles.

## References

- ADR-001 (event-sourcing)
- ADR-002 (Dempster-Shafer fusion is deterministically replayable)
- ADR-005 (HMAC dual-secret rotation primitive)
- Architecture V1 P2 — doctrines (especially D14, D15, D16, D17)
- Architecture V4 P14 — canonical data model
- `docs/security/threat-model.md` — STRIDE coverage and Gap G1
