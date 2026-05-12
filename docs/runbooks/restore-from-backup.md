# Runbook — restore from backup (RTO/RPO commitments + DR drill)

Status: **In force from COMP-5.**
Author: RÉCOR infrastructure-engineering.
Companion: `docs/runbooks/restore-database-from-backup.md` (DOC-3) is the
detailed step-by-step procedure for a production restore. This document
defines the **commitments**, the **drill** that proves them, and the
**quarterly cadence** that keeps the commitments honest.

If you arrive here from an incident, jump straight to
[restore-database-from-backup.md](restore-database-from-backup.md) —
that is the procedure. Come back here when the incident is closed to
record the observed RTO against the commitment in the quarterly drill
template at `docs/compliance/dr-drill-template.md`.

## Why this document exists

Two doctrines pin this runbook into existence:

- **D14 fail-closed** — a backup that has never been restored is not a
  backup; it is a hope. The drill exercises the actual recovery path
  end-to-end on a regular cadence so the moment we need it in anger we
  already know it works.
- **D16 observability** — the platform's stated RTO + RPO are the
  measurements that prove the commitment. The drill produces those
  measurements; this runbook is where they are recorded.

## Recovery objectives

| Objective | Target | Observed baseline | Notes |
|---|---|---|---|
| **RTO** (recovery time objective) | **< 30 min** for the operational stack to accept traffic after a confirmed full-data-loss event | _TBD — first quarterly drill_ | Measured by `scripts/dr-drill.sh`; the value is printed at the end of every drill run. |
| **RPO** (recovery point objective) | **< 15 min** of data potentially lost in the worst case | _TBD — depends on backup cadence_ | Bounded by the WAL-archive flush window. Until the production backup pipeline (R-OPS-BACKUP) lands, the operational RPO is "the last hourly logical backup", i.e. up to 60 minutes. |

The first measured RTO and RPO from a real drill update both the
"Observed baseline" column above AND the corresponding row in
`docs/runbooks/restore-database-from-backup.md` § Recovery objectives.
If the observed RTO exceeds the target, open a follow-up ticket and
adjust the commitment OR fix the procedure — pick exactly one. Quietly
moving the target is a doctrine violation.

## Backup posture

The retention + immutability story is documented in
[`docs/compliance/data-retention.md`](../compliance/data-retention.md)
(COMP-2). The salient parts for restore planning:

- **Event log** (`declaration_events`, `verification_cases`) is
  retained **forever** and is the load-bearing record. A restore that
  recovers the event log + the projections it derives is sufficient to
  reconstruct the platform's state.
- **Outbox tables** retain **30 days post-`dispatched_at`**. Anything
  pruned before the restore target is unrecoverable from a backup; it
  is also irrelevant (it was already delivered).
- **DLQs** are retained forever — never pruned.
- **Idempotency tokens** auto-expire by `expires_at`; a restore that
  recovers an idempotency table older than ~24 h is fine to leave in
  place (stale rows are filtered by the query predicate).

### Backup cadence (initial)

Until R-OPS-BACKUP delivers the production pipeline (pgBackRest +
continuous WAL archive to an S3-compatible bucket), the
operational backup is:

| Stage | Cadence | Mechanism | Retention |
|---|---|---|---|
| Pre-production logical backup | **hourly** | `pg_dump -Fc` via a CronJob writing to a tier-1 object store | 30 days |
| Pre-production base + WAL | **deferred to R-OPS-BACKUP** | pgBackRest sidecar | per R-OPS-BACKUP design |
| Production base | **deferred to R-OPS-BACKUP** | pgBackRest sidecar; nightly base + continuous WAL | 90 days base, 7 days WAL beyond the latest base |

The DR drill exercises the `pg_dump -Fc` path because that is the path
that exists today. When R-OPS-BACKUP lands, the drill is updated to
exercise the production pipeline (pgBackRest restore + WAL replay) —
the drill script is the contract that proves whatever backup mechanism
is in place actually restores.

## The drill — `scripts/dr-drill.sh`

A drill is a controlled, destructive rehearsal of the restore
procedure. It is not optional and it is not "best-effort"; if any
phase fails the drill exits non-zero, and the failure blocks the
quarterly attestation until the underlying issue is resolved
(doctrine **D14 fail-closed**).

### What the drill does

1. Brings up the D↔V loop stack
   (`services/declaration/docker-compose.integration.yaml`).
2. Submits a deterministic, signed declaration as seed data.
3. Captures the seed's `GET /v1/declarations/{id}` response so the
   post-restore comparison is byte-exact.
4. Takes a `pg_dump -Fc` snapshot of both Postgres databases.
5. Simulates **full data loss** — `docker compose` is used to remove
   both postgres services and their named volumes.
6. Brings up fresh, empty Postgres instances.
7. `pg_restore` from the snapshot into each fresh DB.
8. Asserts both services' `/healthz` and `/readyz` return 200.
9. Re-fetches the seeded declaration via GET and asserts byte-for-byte
   equality with the pre-loss capture.
10. Prints `RTO observed: Xs` — the wall time from the destructive
    event to the post-restore byte-equality assertion passing.

### Running the drill

```bash
# From the repository root, with Docker available locally:
bash scripts/dr-drill.sh
```

Optional environment variables (defaults shown):

| Variable | Default | Purpose |
|---|---|---|
| `DR_DRILL_SNAPSHOT_DIR` | `mktemp -d` | Where the pg_dump snapshots + diff artefacts land. Override to keep the evidence in a known location. |
| `DR_DRILL_DECL_HOST`, `DR_DRILL_DECL_PORT` | `127.0.0.1`, `8080` | Declaration service host:port (matches the compose port mapping). |
| `DR_DRILL_VER_HOST`, `DR_DRILL_VER_PORT` | `127.0.0.1`, `8081` | Verification engine host:port. |

On a successful drill, the stack is left **running** — the operator
can poke at the recovered platform before tearing it down. On
failure, the script dumps the last 40 log lines of each service to
stderr and preserves the snapshot directory.

### Reading the result

The last line of a successful drill is the operative measurement:

```text
RTO observed: 47s
```

That number goes into:

1. The current quarter's drill record at
   `docs/compliance/dr-drill-YYYY-Qn.md` (copy from
   `dr-drill-template.md`).
2. The "Observed baseline" column of the table at the top of this
   document, if it is better OR worse than the existing baseline.
3. The corresponding row in
   `docs/runbooks/restore-database-from-backup.md` if the trend has
   diverged from the commitment.

If `RTO observed > target`, file a follow-up ticket. Do not move the
target without an ADR.

## Quarterly drill cadence

The drill is a **quarterly required exercise**. The operator on rota
for the quarter is responsible for:

1. Running the drill against a recent build of `main`.
2. Filing the result in
   `docs/compliance/dr-drill-YYYY-Qn.md` from the template.
3. Opening follow-up tickets for any deviation from the commitment.
4. Signing off the quarter's attestation in the
   `docs/compliance/dr-drill-YYYY-Qn.md` document.

Calendar reminder: `[CITATION NEEDED: ticketing-system]` — a recurring
ticket / calendar event will be configured when the project's
canonical ticketing system is chosen (Linear / Asana / GitHub Projects
are the candidates). Until then, the cadence is enforced manually by
the platform-ops lead.

A **missed drill** is itself a finding: the next time the drill runs
it must include a note explaining the gap, and the quarterly
attestation is incomplete until the gap is acknowledged.

## CI coverage (nightly smoke)

The drill also runs on a schedule against `main` via
`.github/workflows/dr-drill-smoke.yaml`. This is a **non-required**
check (per the OBS-2 deferral pattern documented in
`.github/workflows/observability-smoke.yaml`); it promotes to required
after **10 consecutive green runs**. Until then, a red CI drill is a
signal to investigate but does not block merges; a red **quarterly**
drill always blocks the attestation.

The nightly drill exercises the same script, so any rot in the restore
path is caught long before the quarterly drill needs to find it.

## Related documents

- [restore-database-from-backup.md](restore-database-from-backup.md) —
  the actual production restore procedure (DOC-3).
- [`docs/compliance/data-retention.md`](../compliance/data-retention.md)
  — retention policy that informs the RPO.
- [`docs/compliance/dr-drill-template.md`](../compliance/dr-drill-template.md)
  — quarterly drill record template.
- [incident-response-template.md](incident-response-template.md) — the
  incident wrapper a real (non-drill) restore lives inside.
- [observability-prod-stack.md](observability-prod-stack.md) — where
  to watch the recovery's health metrics in real time.

## Open items

- [ ] R-OPS-BACKUP: deliver the production backup pipeline
  (pgBackRest + S3-compatible bucket + continuous WAL). Until then,
  the production RPO commitment is **aspirational, not measured**.
- [ ] First quarterly drill: populate "Observed baseline" for both
  RTO and RPO.
- [ ] Ticketing-system citation for the calendar reminder.
- [ ] Promote `dr-drill-smoke.yaml` to required after 10 consecutive
  green nightly runs.
