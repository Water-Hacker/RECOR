# Runbook — DLQ inundation

When > 100 rows accumulate in either outbox dead-letter queue inside a
short window, or when the DLQ is growing faster than it drains. The
DLQ is the platform's fail-closed boundary for D ↔ V replication: a
row lands there only after exhausting all dispatch attempts.

## Trigger

Any of:

- `recor_outbox_dlq_size{db="declaration"}` or
  `recor_outbox_dlq_size{db="verification-engine"}` > 100
- DLQ size growing > 10 rows / minute for > 5 minutes
- Operator-reported failure in the D ↔ V loop with `last_error` strings
  visible on DLQ rows
- A page from the alert `RecorOutboxDlqInundation`

A small steady-state DLQ count (< 10) is normal — single transient
failures that exhausted attempts. The trigger here is **inundation**:
many rows accumulating, indicating a systemic problem upstream.

## Prerequisites

- Admin principal listed in the service's `ADMIN_PRINCIPALS` env
  (see `services/declaration/src/api/dlq.rs` § "Authorisation")
- `kubectl` against the production context
- `psql` v17 (matches the deployed Postgres major version)
- Access to the appropriate `DATABASE_URL` for the implicated service
  (read via `kubectl -n recor get secret recor-<svc>-db -o yaml`)
- Read access to the corresponding Grafana dashboard

> The DLQ admin endpoints **require** the calling principal's subject
> to appear in `ADMIN_PRINCIPALS`. An empty allowlist disables the
> endpoint entirely (503). If you receive 503 from a DLQ admin
> endpoint, your principal is not configured; ask the security team
> rather than escalating the admin list under pressure.

## Procedure

The procedure has three phases:

A. **Diagnose** — figure out why rows are dead-lettering (do NOT
   replay first; replaying without fixing the cause just refills the
   DLQ).

B. **Fix the cause** — common causes have specific runbooks; once
   fixed, new submissions stop dead-lettering.

C. **Replay** — once the cause is fixed, replay the accumulated DLQ
   rows so legitimate declarations are not lost.

### Phase A — Diagnose

#### Step A.1 — Identify which DLQ is implicated

```bash
# From a host with kubectl access:
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" -c "SELECT COUNT(*) FROM outbox_dlq;"

kubectl -n recor exec deploy/verification-engine -- \
  psql "${DATABASE_URL}" -c "SELECT COUNT(*) FROM verification_outbox_dlq;"
```

Or via the admin endpoints (HTTPS, requires admin principal):

```bash
# Declaration's DLQ
curl -sf -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
  https://api.recor.cm/v1/internal/outbox-dlq?limit=5 \
  | jq '{total, items: (.items | map({id, event_type, last_error}))}'

# Verification engine's DLQ
curl -sf -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
  https://verify.recor.cm/v1/internal/outbox-dlq?limit=5 \
  | jq '{total, items: (.items | map({id, event_type, last_error}))}'
```

The endpoint surface is documented at
`services/declaration/src/api/dlq.rs` (declaration) and
`services/verification-engine/src/api/dlq.rs` (verification-engine).
Both expose `GET /v1/internal/outbox-dlq` and `POST
/v1/internal/outbox-dlq/{id}/replay`.

#### Step A.2 — Sample the failure reasons

```bash
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" -c "
    SELECT last_error, COUNT(*) AS n
    FROM outbox_dlq
    GROUP BY last_error
    ORDER BY n DESC
    LIMIT 10;
  "
```

(Substitute `verification_outbox_dlq` for the V-engine DLQ.)

The output buckets rows by error string. Most inundation events are
single-cause; if one bucket is > 80 % of the rows, that's your lead.

#### Step A.3 — Map error → cause

| `last_error` pattern | Likely cause | Next runbook |
|---|---|---|
| `connection refused` / `dial tcp ...` | Downstream service unreachable | Check the implicated service's pod health; if it's a partner service: [bunec-adapter-outage](bunec-adapter-outage.md) |
| `401 Unauthorized` / `invalid HMAC` | HMAC secret mismatch between signer and verifier | [hmac-secret-rotation](hmac-secret-rotation.md) — almost always means rotation was botched |
| `OIDC verifier error` / `JWKS unreachable` | Auth side-channel broken | [oidc-issuer-outage](oidc-issuer-outage.md) |
| `503 Service Unavailable` from upstream | Downstream rate-limited or in degraded mode | Inspect the downstream service per [oncall-triage-tree](oncall-triage-tree.md) |
| `timeout exceeded` | Downstream slow but reachable | Check `recor_http_client_duration_seconds` for the implicated client |
| `database is locked` / `pg_pool exhausted` | Local DB resource exhaustion | See § "DB resource exhaustion" below; not partner-side |
| `schema validation failed` / `400 Bad Request` | Downstream rejected the payload — wire-format mismatch | Compare the deployed image SHAs across services (see Step A.4) |
| Empty / unspecified | Bug — the relay should always set `last_error`; file a ticket | Continue investigation but document the empty-error case as a follow-up |

#### Step A.4 — Check wire-format compatibility

If errors are `400 Bad Request` or `schema validation failed`, both
services must be on monorepo commits where the canonical-form parity
rule still holds (see `applications/declarant-portal/CLAUDE.md` §
"The canonical-form parity rule").

```bash
for svc in declaration verification-engine; do
  kubectl -n recor get deploy "${svc}" \
    -o jsonpath='{.spec.template.spec.containers[0].image}'
  echo
done
```

If the tags differ AND span a commit that touched canonical form,
roll forward (or back) to a matching pair via
[deploy-new-version](deploy-new-version.md) or
[rollback-deployment](rollback-deployment.md).

### Phase B — Fix the cause

Apply the runbook identified in A.3. Once the cause is fixed, **new**
submissions stop dead-lettering. Confirm this BEFORE Phase C:

```bash
# Wait 2 minutes, then re-check growth rate:
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" -c "
    SELECT COUNT(*) FROM outbox_dlq
    WHERE dead_lettered_at > NOW() - INTERVAL '2 minutes';
  "
```

The count should be 0 (or very low). If new rows are still
dead-lettering at the same rate, the cause is not yet fixed; do not
proceed to replay.

### Phase C — Replay (only after the cause is fixed)

#### Step C.1 — Dry-run: list what you're about to replay

```bash
curl -sf -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
  'https://api.recor.cm/v1/internal/outbox-dlq?limit=100' \
  | jq '.total, .items | map({id, event_type, dispatch_attempts, last_error}) | .[0:5]'
```

Inspect a sample. Confirm:

- These are events you want to replay (not e.g. rows generated by a
  bug that you're going to fix in code; replaying those just refills
  the DLQ).
- The `event_type` and `aggregate_id` look right.
- The `last_error` matches the pattern you just fixed.

#### Step C.2 — Replay one row first (verification)

```bash
SAMPLE_ID=<id-from-the-list>
curl -sf -X POST -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
  "https://api.recor.cm/v1/internal/outbox-dlq/${SAMPLE_ID}/replay" \
  | jq .
# Expected: {"id": "...", "replayed": true}
```

Then wait 30 s and verify the row dispatched successfully:

```bash
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" -c "
    SELECT id, dispatched_at, dispatch_attempts
    FROM outbox WHERE id = '${SAMPLE_ID}';
  "
```

Expected: `dispatched_at IS NOT NULL`. The row was moved back to
outbox by `replay`, then the relay successfully dispatched it.

If `dispatched_at` is still NULL after 30 s — the cause is NOT fully
fixed. Stop and return to Phase A.

If it dispatches BUT then dead-letters again within a minute — same
conclusion; stop and return to Phase A.

#### Step C.3 — Bulk replay

Replay the rest in a controlled loop, paced to avoid overwhelming the
downstream:

```bash
# Replay up to 100 at a time, pacing 1/s to give the relay's
# dispatch loop room.
curl -sf -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
  'https://api.recor.cm/v1/internal/outbox-dlq?limit=100' \
  | jq -r '.items[].id' \
  | while read id; do
      echo "Replaying ${id}"
      curl -sf -X POST -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
        "https://api.recor.cm/v1/internal/outbox-dlq/${id}/replay" >/dev/null \
        || echo "  FAILED for ${id}"
      sleep 1
    done
```

If more than 100 rows are dead-lettered, repeat the page after each
batch.

For very large DLQs (> 1000 rows), use the direct SQL path via
`kubectl exec`:

```bash
# Move rows back atomically. Requires DB-level access (not the admin
# endpoint), so it is appropriate only when the inundation is large
# enough that one-at-a-time replay is impractical.
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" <<'SQL'
BEGIN;
WITH replayed AS (
  DELETE FROM outbox_dlq
  WHERE dead_lettered_at > NOW() - INTERVAL '24 hours'
  RETURNING *
)
INSERT INTO outbox (id, event_id, event_type, event_version,
                    aggregate_type, aggregate_id, partition_key,
                    payload, created_at)
SELECT id, event_id, event_type, event_version,
       aggregate_type, aggregate_id, partition_key,
       payload, created_at
FROM replayed;
COMMIT;
SQL
```

The atomic CTE pattern is safe to interrupt: if the transaction is
killed mid-way, no rows move; if it commits, all move. Idempotency
(D13) holds because the outbox row's `id` is its primary key.

### Step C.4 — Confirm drain

```bash
# Wait 2 minutes after bulk replay, then:
kubectl -n recor exec deploy/declaration -- \
  psql "${DATABASE_URL}" -c "
    SELECT
      (SELECT COUNT(*) FROM outbox WHERE dispatched_at IS NULL) AS outbox_pending,
      (SELECT COUNT(*) FROM outbox_dlq) AS dlq_remaining;
  "
```

Both should be approaching 0. Outbox-pending is normal at low values
(rows being processed). DLQ should be empty or near it.

## DB resource exhaustion (sub-procedure)

If `last_error` strings indicate DB resource exhaustion rather than
downstream issues:

1. Check pool size: `kubectl -n recor logs deploy/declaration | grep
   "pool"`
2. Increase pool size in the deployment env (`DB_MAX_CONNECTIONS`) —
   PR + deploy via [deploy-new-version](deploy-new-version.md).
3. Or scale Postgres up (more memory / vCPU) — coordinated via the SRE
   lead; this is a separate runbook (cluster-resize) per the SRE team.

## Verification

The incident is resolved when:

- `recor_outbox_dlq_size` is below the alert threshold (< 100) for
  both DBs
- New submissions in the 5 minutes since fix do NOT generate new DLQ
  rows
- A canary submission (production sample data) completes end-to-end:
  declaration accepted → outbox dispatched → verification engine
  consumed → case_id resolved
- The cause is documented in the incident post-mortem with an action
  item to prevent recurrence
- If the cause was a wire-format mismatch (Step A.4), both services
  are confirmed on monorepo-aligned image tags

## Rollback

Replay is the rollback for dead-letter: if a row is replayed by
accident and you wanted it left in the DLQ for forensic reasons, it
is now in `outbox` instead of `outbox_dlq`. Move it back manually:

```sql
BEGIN;
WITH moved AS (
  DELETE FROM outbox WHERE id = '<id>' RETURNING *
)
INSERT INTO outbox_dlq (id, event_id, event_type, event_version,
                        aggregate_type, aggregate_id, partition_key,
                        payload, created_at, dead_lettered_at,
                        dispatch_attempts, last_error)
SELECT id, event_id, event_type, event_version,
       aggregate_type, aggregate_id, partition_key,
       payload, created_at, NOW(), dispatch_attempts,
       'manually preserved post-replay'
FROM moved;
COMMIT;
```

If a bulk replay moved rows that you wanted preserved, the atomic CTE
in Step C.3 can be inverted (swap `outbox_dlq` ↔ `outbox`).

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md)
- [hmac-secret-rotation](hmac-secret-rotation.md)
- [oidc-issuer-outage](oidc-issuer-outage.md)
- [bunec-adapter-outage](bunec-adapter-outage.md)
- [rollback-deployment](rollback-deployment.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [incident-response-template](incident-response-template.md)
