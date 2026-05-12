# Runbook — restore a database from backup

How to recover the declaration and / or verification-engine Postgres
databases from backup after a data-loss event. The procedure is the
same shape for both services; differences are called out inline.

## Trigger

Any of:

- Confirmed data corruption (incorrect rows in a critical table:
  `declarations`, `outbox`, `outbox_dlq`, `events`)
- Accidental destructive operation (`TRUNCATE`, `DROP`, mass-`DELETE`
  against the live database)
- Storage-layer failure (volume corruption, hardware loss not covered
  by replication)
- Confirmed compromise where data integrity is suspect

This runbook is invoked rarely. The much more common case — a single
bad row or a small set of malformed events — is handled in code via
event-sourcing replay or admin endpoints, not by a full restore.
Restoring is a last resort because RPO is non-zero.

## Recovery objectives

| | Target | Actual baseline | Notes |
|---|---|---|---|
| **RTO** (recovery time objective) | 4 hours | TBD — measured by R-OPS-DRDRILL | From decision-to-restore until traffic accepted |
| **RPO** (recovery point objective) | 15 minutes | TBD — measured by R-OPS-DRDRILL | The window of data potentially lost |

> The targets above are the architectural commitment (Architecture V5
> P22 § BCP / DR). The "actual baseline" gets filled in by the
> first quarterly DR drill (R-OPS-DRDRILL ticket). Until then, treat
> these as the goals, not measured behaviour.

## Prerequisites

- Write access to the production cluster context (`recor-prod`)
- Read access to the backup bucket (`s3://recor-prod-backups/postgres/`
  in the canonical layout; substitute per your cloud)
- `pg_restore` v17+ on a host with network access to the cluster
- The most recent backup manifest:
  `s3://recor-prod-backups/postgres/MANIFEST.json` (lists timestamped
  base backups + WAL ranges)
- A declared incident: a full database restore is SEV-1, every time
- Sign-off from the data-integrity owner (`@recor/architect-team` for
  declaration; `@recor/verification-team` for verification-engine).
  Do not begin without it.

> The production backup pipeline (pgBackRest sidecar + S3-compatible
> bucket + nightly base + continuous WAL) is delivered by
> **R-OPS-BACKUP** (`docs/PRODUCTION-TODO.md`). Until that ticket
> lands, production backups are TBD. This runbook documents the
> procedure as it will be once R-OPS-BACKUP ships; running it before
> then will fail at Step 2.

## Procedure

### Step 0 — Stop accepting writes (fail-closed)

Before touching the database, **stop new writes**. A restore that
proceeds while writes continue produces a fork: the recovered DB has
data from the backup window, the rejected writes are lost without
trace, and reconciliation is much harder later.

Scale the writer deployments to zero:

```bash
# Declaration service writes; verification-engine writes only to its
# own DB so handle them independently per which DB is being restored.

# If restoring declaration's DB:
kubectl -n recor scale deploy/declaration --replicas=0
# Confirm pods are gone:
kubectl -n recor get pods -l app=declaration

# If restoring verification-engine's DB:
kubectl -n recor scale deploy/verification-engine --replicas=0
kubectl -n recor get pods -l app=verification-engine
```

The declarant portal will return 502 against the now-down API; this
is intentional and visible. Post in `#oncall-recor` that the API is
intentionally offline. D14 (fail-closed): better a known offline than
a silently-forked dataset.

### Step 1 — Identify the restore target

You need to choose **a point in time** to restore to. The default
choice is "the moment immediately before the destructive event."

```bash
# Pull the backup manifest:
aws s3 cp s3://recor-prod-backups/postgres/MANIFEST.json - \
  | jq '.base_backups | sort_by(.timestamp) | .[-5:]'
```

This shows the five most recent base backups. WAL archives cover the
intervals between them and extend continuously up to "approximately
now minus 1 minute" (the WAL flush cadence).

Pick the latest base backup with `timestamp < destructive_event_time`,
plus a target recovery timestamp:

```bash
BASE_BACKUP_ID=<id-from-manifest>
RESTORE_TARGET_TS="YYYY-MM-DD HH:MM:SS+00"   # UTC, just before the event
DB_NAME=declaration                          # or verification-engine
echo "Restoring ${DB_NAME} to ${RESTORE_TARGET_TS} from base ${BASE_BACKUP_ID}"
```

### Step 2 — Dry-run: provision a side-by-side restore target

**Never restore in-place over a live DB without a verified copy first.**
Stand up a parallel Postgres instance, restore to it, validate, then
promote. The DB you are restoring becomes the side-by-side; the live
DB is preserved untouched until promotion (Step 5).

```bash
# Provision a sidecar Postgres pod. The Helm chart's
# `restore-target` value enables an additional StatefulSet with no
# traffic routed to it.
helm -n recor upgrade --reuse-values recor-postgres-${DB_NAME} \
  infrastructure/helm/postgres \
  --set restoreTarget.enabled=true \
  --wait --timeout=10m

# Confirm it came up:
kubectl -n recor get sts postgres-${DB_NAME}-restore
```

> The `restoreTarget.enabled` Helm value is **TBD — depends on
> R-OPS-BACKUP** (where the chart is finalised). Until then, this
> step is run by hand from a bastion using the `pgBackRest` CLI
> against a freshly-provisioned managed-Postgres replica. The shape
> is the same; the exact command surface is bootstrap-ops.

### Step 3 — Restore base + replay WAL (dry-run first)

Restore base backup, then replay WAL up to the chosen target time.

```bash
# Run inside the restore-target pod:
kubectl -n recor exec -it sts/postgres-${DB_NAME}-restore -- bash -lc '
  pgbackrest --stanza=recor-${DB_NAME} \
    --type=time \
    --target="'"${RESTORE_TARGET_TS}"'" \
    --target-action=pause \
    restore
'
```

`--target-action=pause` is the dry-run equivalent: Postgres replays
WAL up to the target, then pauses. You can connect and inspect before
promoting. **Do not skip this.**

### Step 4 — Validate the restored DB

Connect to the restored instance (NOT the live one):

```bash
kubectl -n recor exec -it sts/postgres-${DB_NAME}-restore-0 -- \
  psql -U recor -d ${DB_NAME}
```

Run the validation queries appropriate to the DB:

**For declaration:**

```sql
-- Most recent declaration in the restored data:
SELECT id, submitted_at FROM declarations ORDER BY submitted_at DESC LIMIT 1;

-- Outbox health: no rows from after the target time:
SELECT COUNT(*) FROM outbox WHERE created_at > '<RESTORE_TARGET_TS>';
-- Expected: 0

-- DLQ row count for sanity:
SELECT COUNT(*) FROM outbox_dlq;

-- Event log consistency: every declaration has at least one event:
SELECT COUNT(*) FROM declarations d
  WHERE NOT EXISTS (
    SELECT 1 FROM events e WHERE e.aggregate_id = d.id
  );
-- Expected: 0
```

**For verification-engine:**

```sql
-- Most recent case:
SELECT id, completed_at FROM verifications ORDER BY completed_at DESC LIMIT 1;

-- Verification outbox / DLQ:
SELECT COUNT(*) FROM outbox WHERE created_at > '<RESTORE_TARGET_TS>';
SELECT COUNT(*) FROM verification_outbox_dlq;
```

If validation fails (rows from past the target, missing events,
orphaned declarations) — **stop**. The backup or WAL chain has a gap;
escalate before promoting. Picking an earlier target time is the
typical recovery.

### Step 5 — Promote the restored DB

When validation passes, finalise WAL replay and promote the restored
instance to writable:

```sql
-- Inside the restored DB's psql session:
SELECT pg_wal_replay_resume();
```

Then swap the service to point at the restored instance. The Helm
chart writes `DATABASE_URL` from a Secret; flip the Secret to point at
the restore-target Service:

```bash
kubectl -n recor patch secret recor-${DB_NAME}-db \
  --type='json' \
  -p='[{"op":"replace","path":"/data/DATABASE_URL","value":"'"$(printf 'postgres://recor:%s@postgres-%s-restore.recor.svc:5432/%s?sslmode=require' "${RECOR_DB_PASSWORD}" "${DB_NAME}" "${DB_NAME}" | base64 -w0)"'"}]'
```

> The exact Secret name / structure is **TBD — depends on
> R-OPS-BACKUP** (when the production Helm chart lands). The shape
> is documented here so the operator knows what to look for.

### Step 6 — Bring writers back up

```bash
# For declaration:
kubectl -n recor scale deploy/declaration --replicas=3
# For verification-engine:
kubectl -n recor scale deploy/verification-engine --replicas=3

# Watch them come up:
kubectl -n recor rollout status deploy/${SVC}
```

The new pods read the patched Secret on startup, connect to the
restored DB, and resume traffic.

### Step 7 — Smoke and confirm

```bash
curl -sf https://api.recor.cm/healthz | jq .
curl -sf https://api.recor.cm/readyz  | jq .
# Submit a known-shape declaration through the staging-equivalent
# canary flow if one is wired; otherwise observe production traffic.
```

Watch the dashboards for 30 minutes after restore: any anomaly is the
sign that the recovered state has a subtle inconsistency.

### Step 8 — Reconcile the loss window

Between `RESTORE_TARGET_TS` and the destructive event, real
declarations / verifications were accepted that are NOT in the
restored DB. These are now lost. Three things must follow:

1. **Communicate.** Per the data-integrity owner, notify affected
   declarants (the count is bounded by writes during the loss window).
2. **Audit.** Pull the loss-window timestamp range against any
   downstream consumers (BUNEC, ANIF, etc.) — if a verification result
   was sent to a consumer but is now absent from the local DB, the
   consumer's record diverges from ours.
3. **Document.** The lost-records list goes in the incident
   post-mortem.

### Step 9 — Decommission the old DB only after the new is proven

The old (corrupted / pre-restore) database stays running and
unconfigured for at least 7 days after restore. If a post-restore
issue surfaces and the cause is the restore itself, the old DB is the
forensic record.

Decommission only after the 7-day clock and after the post-mortem
explicitly approves.

## Verification

The restore is complete when ALL of the following are true:

- `kubectl -n recor get pods -l app=<service>` shows all pods `Running`
- Healthz / readyz return 200 against production ingress
- Validation queries (Step 4) pass against the **live** (now-restored)
  DB
- A canary write succeeds and is visible in the DB and in Grafana
- The reconciliation steps (Step 8) are open as tracked tickets
- The old (corrupted) DB is preserved but disconnected
- An incident post-mortem PR is open per
  [incident-response-template](incident-response-template.md)

## Rollback

If the restore itself made things worse (e.g. validation passed in
the side-by-side, but production smoke fails after promotion), revert
the Secret patch in Step 5 to point back at the original (untouched)
DB:

```bash
kubectl -n recor rollout undo deploy/declaration
# Or, if the Secret patch is the only difference, restore the prior
# Secret from the backup taken before the patch:
kubectl -n recor apply -f /tmp/recor-${DB_NAME}-db-pre-restore.yaml
kubectl -n recor rollout restart deploy/${SVC}
```

**Before Step 5, always save the prior Secret to a local file:**

```bash
kubectl -n recor get secret recor-${DB_NAME}-db -o yaml \
  > /tmp/recor-${DB_NAME}-db-pre-restore.yaml
```

The rollback path is only viable if the original DB has not been
touched. That's the reason for Step 0 (stop writes) and Step 9 (don't
decommission for 7 days).

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md)
- [rollback-deployment](rollback-deployment.md)
- [dlq-inundation](dlq-inundation.md)
- [hmac-secret-rotation](hmac-secret-rotation.md)
- [incident-response-template](incident-response-template.md)
- [observability-prod-stack](observability-prod-stack.md)
