# Runbook — DLQ retention policies

**Audit reference:** closes the Audit chain row of the MEDIUM/LOW
summary table — "bridge worker's `fabric_bridge_dlq` retention
undocumented."

This runbook is the canonical reference for how long each
Dead-Letter Queue retains its rows after an entry has either drained
to the sink or been declared permanently dead.

## DLQ inventory

| DLQ table | Service | What ends up here | Retention |
|---|---|---|---|
| `outbox_dlq` | `services/declaration` | Outbox rows where dispatch retries are exhausted | 30 days post-dispatch (success or final-fail) |
| `verification_outbox_dlq` | `services/verification-engine` | Verification-outcome relay rows where retries are exhausted | 30 days post-dispatch |
| `fabric_bridge_dlq` | `apps/worker-fabric-bridge` | Audit-chain events the bridge couldn't anchor on Fabric after `MAX_RETRIES` | 90 days post-final-fail |
| `person_outbox` (relay TBD) | `services/person-service` | Person-event projection rows pending downstream relay | 30 days post-dispatch |
| `entity_outbox` (relay TBD) | `services/entity-service` | Entity-event projection rows pending downstream relay | 30 days post-dispatch |

Two retention tiers in use:
- **30 days** — standard outbox relays. Long enough for a slow consumer
  to catch up; short enough that the operator can detect drift.
- **90 days** — Fabric bridge DLQ. Longer because audit-chain anchoring
  is the long tail of regulatory evidence; an item dead-on-Fabric still
  has manual remediation paths (re-anchor, sovereign override) that an
  operator may invoke a month or two later.

## Why these numbers

**30 days for the standard outbox DLQ.** Aligned with the soft-launch
playbook's "first 30 days under intense observation" window. Any
relay-tail that exceeds 30 days indicates a structural problem (sink
permanently broken; secret rotation missed; signing key revoked) that
demands escalation rather than retention.

**90 days for `fabric_bridge_dlq`.** Fabric's network can have multi-week
governance windows (peer-bring-up, channel-policy change). A DLQ
retention shorter than the network's worst-case governance window
would discard evidence the operator legitimately needs. 90 days is
~one quarter, which covers the longest documented BUNEC governance
window plus a safety margin.

The audit reconciler (`apps/audit-reconciler`, FIND-016) catches the
"event in `declaration_events` not on-chain" case before the bridge DLQ
ever fills up; the bridge DLQ is the safety net for the reconciler
itself missing a row (e.g. a row added between reconciler runs that
fell off the bridge's retry budget).

## Retention enforcement

Each service ships a retention worker that runs on the same cron-tick
as the rest of the platform's hygiene jobs:

```sql
-- declaration outbox + verification outbox: 30 days
DELETE FROM outbox
WHERE dispatched_at IS NOT NULL
  AND dispatched_at < now() - interval '30 days';

-- fabric bridge DLQ: 90 days
DELETE FROM fabric_bridge_dlq
WHERE final_failed_at IS NOT NULL
  AND final_failed_at < now() - interval '90 days';
```

The retention worker is single-threaded across the service replica
set (advisory lock on a well-known `pg_advisory_lock` key). It runs
hourly. Per-table deletes are bounded by `LIMIT 10000` to avoid long
transaction holds; the worker simply runs again next tick if there's
backlog.

## On-call hooks

Three Prometheus alerts (see `alerts/recor-prometheus-rules.yaml`):

- `outbox_dlq_size_high` — page when any DLQ table holds > 100 rows
  not yet dispatched. (Closes the audit's "DLQ inundation alert
  threshold (100) not Prometheus-rule-enforced" finding.)
- `outbox_retention_worker_stuck` — page when the retention worker
  hasn't run for any service in > 2 hours.
- `fabric_bridge_dlq_climbing` — page when `fabric_bridge_dlq` grows
  by > 50 rows / hour (chain anchoring catastrophic).

## Drain procedure (DLQ inundation)

If a DLQ inundates:

1. Triage: query the DLQ to see why rows landed there. Look at the
   `last_error` column — if every row has the same error class
   (`hmac_invalid`, `connection_refused`, `gateway_5xx`), the cause is
   structural.
2. Fix the structural cause (secret rotated; sink up; HMAC
   `iat`-window aligned).
3. Replay: `POST /v1/dlq/{id}/replay` on the relevant service (admin
   allowlist gated). Replay is idempotent — the consumer's idempotency
   key prevents double-effect.
4. Verify drain: `outbox_dlq_size` metric returns to steady-state.
5. If replay itself fails for the entire DLQ, the cause is not yet
   fixed — return to step 1.

## Manual deletion (operator override)

In extraordinary cases, an operator may force-delete DLQ rows that
will never drain (e.g. the sink no longer exists; the event was a
test row). Manual deletion requires:

- A linked incident ticket explaining why the row is unrecoverable.
- An entry in `docs/audit/manual-dlq-deletions.md` with the
  declaration_id / event_id, the operator's principal, and the
  reason.
- Two-operator confirmation (one admin, one cross-team) per D11
  reviewability.

Forge `DELETE FROM` statements do not bypass the COMP-2 trigger on
`declaration_events` — that table is append-only; the DLQ is not.
DLQ deletion does not delete the underlying event; the event remains
in `declaration_events` and can be re-replayed indefinitely by
re-issuing `POST /v1/dlq/{id}/replay`-equivalent ops via the relay
worker.
