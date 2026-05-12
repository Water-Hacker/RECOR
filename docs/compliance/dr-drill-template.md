# Quarterly DR drill record — template

> **How to use this template.** On-call for the quarter copies this
> file to `docs/compliance/dr-drill-YYYY-Qn.md` (where `YYYY` is the
> calendar year and `Qn` is the quarter, e.g. `2026-Q3`), fills in
> every field below, opens a PR titled
> `chore(ops): YYYY Qn DR drill record`, and asks a second on-call to
> review. The PR merges only when every "deviations" item has either
> been resolved or has a tracked follow-up ticket linked.
>
> Do **not** edit this template file directly. Each quarter is its own
> immutable record.

## Header

| Field | Value |
|---|---|
| Quarter | `YYYY Qn` |
| Drill date (UTC) | `YYYY-MM-DD HH:MM` |
| Operator | `@github-handle` |
| Reviewer | `@github-handle` |
| Build under test | `commit-sha` (paste from `git rev-parse HEAD` at the time of the drill) |
| Environment | `dev` / `staging` / `pre-prod` (production drills require ADR + change-window) |
| Drill mechanism | `scripts/dr-drill.sh` (or document any deviation) |

## RTO observation

| Metric | Target | Observed | Δ vs target | Status |
|---|---|---|---|---|
| RTO (loss → traffic-ready) | < 30 min | `Xs` | `±Ys` | ✅ pass / ❌ fail |
| Total drill wall-clock | n/a | `Xs` | n/a | — |

Paste the final `RTO observed: Xs` line from the drill output below:

```text
<paste output>
```

## RPO observation

| Metric | Target | Observed | Notes |
|---|---|---|---|
| RPO (worst-case data loss window) | < 15 min | `<measured/estimated>` | Bounded by the backup mechanism's flush cadence. With `pg_dump`-only, the operational RPO is "time since last hourly dump"; with WAL archiving (R-OPS-BACKUP), the RPO is "time since last WAL flush" (~1 min). |

If the RPO was estimated rather than measured (e.g. because R-OPS-BACKUP
has not landed), say so explicitly. An estimated RPO is not a
substitute for a measured one once the production pipeline is live.

## Deviations from procedure

For each step in `scripts/dr-drill.sh` that did **not** complete as
documented, record the deviation here. "Procedure" is the script + the
companion runbook at
`docs/runbooks/restore-database-from-backup.md`.

| Phase | Expected | Observed | Severity | Follow-up |
|---|---|---|---|---|
| _e.g. Phase 7 — pg_restore_ | _restore in ~5s_ | _restore took 73s on the verification DB_ | _low_ | _ticket `OPS-123`_ |

If there were zero deviations, write "No deviations." Do not leave the
section empty.

## Side-channel observations

Things that surfaced during the drill that are not strictly drill
findings but should be tracked:

- Stack startup time after fresh volume creation: `<seconds>` (baseline
  for future drills).
- Anything noisy in the service logs that didn't fail the drill but
  warrants investigation.
- Anything in the dashboards (Grafana / Prometheus) that flagged or
  shifted during the drill window.

## Attestation

By co-signing this record, the operator and reviewer attest that:

- [ ] The drill was run against the build named above, on the date
  named above.
- [ ] The drill script exited zero (or, if it did not, the failure is
  documented under "Deviations" and a follow-up ticket is linked).
- [ ] Every deviation has either been resolved or has a tracked
  follow-up.
- [ ] The "Observed baseline" columns in
  `docs/runbooks/restore-from-backup.md` and (if applicable)
  `docs/runbooks/restore-database-from-backup.md` have been updated
  to reflect this drill's measurements, if they materially differ
  from the existing baseline.

Signatures (use the PR's two-reviewer signoff):

| Role | Handle | Date |
|---|---|---|
| Operator | `@github-handle` | `YYYY-MM-DD` |
| Reviewer | `@github-handle` | `YYYY-MM-DD` |

## Related documents

- `docs/runbooks/restore-from-backup.md` — RTO/RPO commitments + drill spec.
- `docs/runbooks/restore-database-from-backup.md` — production restore procedure.
- `docs/compliance/data-retention.md` — retention policy that bounds the RPO.
- `scripts/dr-drill.sh` — the drill itself.
