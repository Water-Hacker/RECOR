# Runbook — Declaration staleness watcher (TODO-005)

**Audit reference:** closes TODO-005 in `TODOS.md` — FATF R.24 §c.24.8
fn 29 (BO data MUST be updated within one month of any change; FATF
benchmark: 30 days).

## What this is

A periodic Postgres-side scan of the `declarations` projection that
counts rows where the declarant-asserted `last_event_observed_at` is
older than the staleness threshold (default 30 days) AND no Amend /
Correct event has landed since. Each cycle increments the
`recor_declaration_staleness_observed_total` Prometheus metric;
operators alert + triage from there.

Implementation: `services/declaration/src/infrastructure/staleness.rs`.

## Configuration

| Env var | Default | Meaning |
|---|---|---|
| `STALENESS_INTERVAL_SECONDS` | `0` (DISABLED) | How often to scan. Recommended: 3600 (hourly). |
| `STALENESS_THRESHOLD_DAYS` | `30` | Rows older than this many days are flagged. FATF benchmark is 30. |
| `STALENESS_MAX_PER_CYCLE` | `10000` | Hard cap on rows counted per cycle (defence against pathological scans). |

The worker is **disabled by default**. Production deployments MUST
set `STALENESS_INTERVAL_SECONDS` explicitly. Operators set this once
the BO-DTO layer is populating `last_event_observed_at` (PR-FATF-4.B
follow-up) and the projection has had a chance to ingest declarant
attestations of the BO change date.

## When the metric fires

`recor_declaration_staleness_observed_total` > 0 on any scan cycle
indicates one or more declarations have crossed the 30-day FATF
benchmark without an update. The associated alert rule (deferred
to a follow-up PR alongside `alerts/recor-prometheus-rules.yaml`)
pages operators with severity `ticket` (not `page`) because the
right response is typically a notification to the declarant + a
back-office triage cycle, not an on-call incident.

## Operator response

1. **Triage.** Query the projection directly:

   ```sql
   SELECT
       declaration_id,
       declarant_principal,
       entity_id,
       last_event_observed_at,
       amended_at,
       NOW() - last_event_observed_at AS staleness
   FROM declarations
   WHERE last_event_observed_at IS NOT NULL
     AND last_event_observed_at < NOW() - INTERVAL '30 days'
     AND (amended_at IS NULL OR amended_at < last_event_observed_at)
     AND superseded_at IS NULL
   ORDER BY last_event_observed_at ASC
   LIMIT 100;
   ```

2. **Notify the declarant.** The notification dispatcher is out of
   scope for v1 (no email/SMS infrastructure yet); the v1 operator
   uses the principal subject from the query result to drive an
   out-of-band notification (CRM / phone / letter).

3. **Track repeat offenders.** A declarant whose declarations
   persistently appear here is a candidate for the sanctions
   workflow (TODO-004 — proportionate, dissuasive, effective per
   FATF c.24.13). The sanctions workflow is a separate subsystem.

4. **Adjust threshold during rollover.** If a new rollover surfaces
   pre-existing stale declarations (declarants haven't yet populated
   the new field), temporarily raise `STALENESS_THRESHOLD_DAYS` to
   smooth the bow wave. Restore to 30 once the declarant cohort has
   caught up.

## Rollback

Set `STALENESS_INTERVAL_SECONDS=0` and rolling-restart. The worker
exits at the next iteration boundary. The migration (`0010`) is
forward-only; rollback drops the column with:

```sql
ALTER TABLE declarations DROP COLUMN IF EXISTS last_event_observed_at;
DROP INDEX IF EXISTS idx_declarations_staleness;
```

## Related

- TODO-005 in `TODOS.md`
- ADR-0010 (FATF cascade + adequacy claims) — declaration-side
  context for why `last_event_observed_at` matters alongside the
  adequacy_claims block
- `services/declaration/src/infrastructure/retention.rs` — the
  sibling tokio-task worker that prunes the outbox; same lifecycle
  pattern
- Future: alert rule `declaration_staleness_observed` in
  `alerts/recor-prometheus-rules.yaml`
- Future: TODO-004 (sanctions workflow) consumes this signal as a
  staleness-driven escalation trigger
