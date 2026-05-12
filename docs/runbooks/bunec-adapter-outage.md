# Runbook — BUNEC adapter outage

The verification engine's Stage 2 (identity) adapter to the BUNEC
business register is unreachable, slow, or returning errors. This
runbook covers detection, fallback behaviour, and recovery.

## Context

BUNEC is Cameroon's national business register. Stage 2 of the
verification pipeline queries BUNEC to confirm that declared entities
exist with the declared identifiers, and that the declared beneficial
owners match the register's view.

Today (2026-05-12) Stage 2 is backed by `PostgresMockBunec` — a
mock loaded with fixture data. The real BUNEC adapter is delivered by
**R-VER-1** (`docs/PRODUCTION-TODO.md`), gated on a government data
agreement. Until that ticket ships:

- "BUNEC unreachable" in production is currently equivalent to "the
  postgres-mock-bunec database is unreachable," which is an
  internal-Postgres issue (treat as a database incident).
- The fail-policy lever (`BUNEC_FAIL_POLICY=fail_open | fail_closed`)
  is defined in R-VER-1's interface; the trait wiring already lives
  at `services/verification-engine/src/application/port.rs`.

This runbook documents the procedure as it will be once R-VER-1 ships.
Sections that depend on R-VER-1 inline are flagged.

## Trigger

Any of:

- Stage 2 latency in Grafana → `RECOR / Verification stages` panel >
  2 s p99 sustained for 5 minutes
- BUNEC call error rate (`recor_bunec_calls_total{outcome="error"}` /
  total) > 5 % for 5 minutes
- The BUNEC circuit breaker opens (logs:
  `bunec circuit open at <timestamp>`) — this is the fallback path
  invoked by R-VER-1's adapter
- The BUNEC operations team has informed RÉCOR of a planned or
  unplanned outage
- Alert `RecorBunecAdapterDown` fires

## Prerequisites

- `kubectl` against production
- Read access to Grafana production tenant
- Contact for the BUNEC operations team (per the data-sharing
  agreement; not committed in-repo)
- Knowledge of the configured fail-policy:
  ```bash
  kubectl -n recor get deploy verification-engine \
    -o jsonpath='{.spec.template.spec.containers[0].env[?(@.name=="BUNEC_FAIL_POLICY")].value}'
  echo
  ```
  Expected: `fail_closed` in production, `fail_open` in dev. R-VER-1's
  brief documents this.

## Procedure

### Step 1 — Confirm the outage

```bash
# Per R-VER-1, BUNEC config is BUNEC_API_BASE_URL.
# Read the configured endpoint:
BUNEC_URL=$(kubectl -n recor get deploy verification-engine \
  -o jsonpath='{.spec.template.spec.containers[0].env[?(@.name=="BUNEC_API_BASE_URL")].value}')
echo "BUNEC_API_BASE_URL=${BUNEC_URL}"

# Probe from a service pod (the network path the adapter uses):
kubectl -n recor exec deploy/verification-engine -- \
  curl -sSf -m 10 "${BUNEC_URL}/healthz" \
  | jq . \
  || echo "BUNEC healthz failed"
```

> Until R-VER-1 ships, `BUNEC_API_BASE_URL` is unset — the deployed
> adapter is `PostgresMockBunec`. The probe above will print empty;
> instead, probe the mock-BUNEC Postgres:
>
> ```bash
> kubectl -n recor exec deploy/verification-engine -- \
>   psql "${BUNEC_MOCK_DATABASE_URL}" -c "SELECT 1;"
> ```
>
> If the mock DB is down, it is a database incident — see the
> declaration-DB triage path and adapt to the V-engine DB.

If healthz fails, BUNEC is down or unreachable. If it succeeds but
verification is still slow / failing, the path is application-layer
(specific endpoints failing, auth, rate limits) — proceed to Step 2.

### Step 2 — Determine the failure pattern

```bash
# Sample the recent BUNEC error spans from the V-engine logs:
kubectl -n recor logs deploy/verification-engine --tail=300 \
  | grep -i "bunec" \
  | tail -50
```

Or in Grafana, filter Loki for `{app="verification-engine"} |= "bunec"`
over the last 30 minutes.

Map error pattern → cause:

| Pattern | Likely cause | Action |
|---|---|---|
| `connection timed out` / `dial tcp` | BUNEC TCP unreachable | Likely BUNEC-side outage; engage their on-call |
| `429 Too Many Requests` | RÉCOR is over BUNEC's rate limit | Check `recor_bunec_calls_total` rate; scale down V-engine OR negotiate a higher rate cap with BUNEC team |
| `401` / `403` | BUNEC credential expired / revoked | Rotate the BUNEC API key — see Step 4 |
| `500` / `502` / `503` from BUNEC | BUNEC-side error | BUNEC operations problem; engage their team |
| `circuit breaker open` | Local circuit breaker tripped after 5 consecutive failures (R-VER-1 spec) | The circuit is the platform fail-closing on a downstream issue — root cause is upstream, NOT the breaker |
| `certificate expired` | BUNEC TLS cert expired (or our trust store stale) | Confirm against `openssl s_client`; engage BUNEC if their cert; rotate our trust store if ours |

### Step 3 — Confirm the fallback is behaving correctly

R-VER-1's adapter, on circuit-open, emits a `vacuous BPA` for Stage 2
and continues the pipeline. The case still resolves — but with no
identity evidence. Per fail-policy:

- `fail_closed` (production default): the case routes to **red lane**
  on absent identity evidence. This is the safe behaviour. The
  declaration is flagged for manual review; no false-positive admission
  to the registry.
- `fail_open` (dev / staging only): the case proceeds with vacuous
  Stage 2; downstream stages still contribute; the lane is whatever
  the rest of the pipeline computes.

Confirm the fallback in telemetry:

```bash
# Cases routed to red lane with Stage 2 = vacuous:
kubectl -n recor exec deploy/verification-engine -- \
  psql "${DATABASE_URL}" -c "
    SELECT COUNT(*)
    FROM verifications
    WHERE created_at > NOW() - INTERVAL '10 minutes'
      AND lane = 'red'
      AND stage_outcomes->>'stage_2'  IS NOT NULL
      AND (stage_outcomes->'stage_2'->>'evidence_summary') ILIKE '%bunec%';
  "
```

> The exact JSON path depends on the schema in R-VER-1; the example
> here is illustrative until the schema lands. Substitute the actual
> path. The intent is to count cases that fell through to fallback.

If the count is ≥ 1 and growing, the fallback is operating. The
platform is degraded but **not unsafe**. D14 (fail-closed) is intact.

If the count is 0 BUT cases are failing entirely (500s from
`/v1/verifications`), the fallback is NOT being invoked — that is a
bug in the adapter, escalate to `@recor/verification-team` and
treat as code defect, not pure operational issue.

### Step 4 — Specific sub-procedures

#### 4a. BUNEC credential rotation

If errors are 401 / 403:

```bash
# Rotate the BUNEC API key. The new key is provisioned by BUNEC; we
# update our Secret and rollout.
# The Secret name is recor-bunec-creds (per the Helm chart values).
NEW_KEY=<the-new-key-from-bunec>
kubectl -n recor patch secret recor-bunec-creds \
  --type='json' \
  -p='[{"op":"replace","path":"/data/BUNEC_API_KEY","value":"'"$(echo -n "${NEW_KEY}" | base64 -w0)"'"}]'

# Rollout to pick up the new key:
kubectl -n recor rollout restart deploy/verification-engine

# Verify:
kubectl -n recor exec deploy/verification-engine -- \
  curl -sSf -H "Authorization: Bearer ${NEW_KEY}" "${BUNEC_URL}/healthz" | jq .
```

> The Secret name and structure are **TBD — depends on R-VER-1**.

#### 4b. Rate-limit pressure relief

If errors are 429:

1. Inspect the rate-of-calls metric in Grafana
   (`recor_bunec_calls_total`).
2. If it's a spike — scale verification-engine down to reduce the
   concurrent BUNEC calls:
   ```bash
   kubectl -n recor scale deploy/verification-engine --replicas=1
   ```
   This is a temporary measure; verification throughput will drop. The
   pipeline's queue (the D ↔ V outbox) will absorb the backlog —
   monitor DLQ growth per [dlq-inundation](dlq-inundation.md).
3. Negotiate a higher rate ceiling with the BUNEC operations team or
   inspect whether a particular case-pattern is making N calls per
   case when 1 would suffice (code defect).

#### 4c. Planned BUNEC outage

If BUNEC has informed RÉCOR of a planned outage:

1. Pre-announce in `#oncall-recor` and on the public status page.
2. Confirm `BUNEC_FAIL_POLICY=fail_closed` (default; refusing
   admission during a planned BUNEC outage is the safe posture for
   sovereign-grade data integrity).
3. Expect a red-lane spike for the outage duration. The cases route
   to the manual-review queue; the queue is processed after BUNEC
   recovery via re-verification (see Step 5).

### Step 5 — Recovery

When BUNEC is reachable again:

1. Probe (Step 1) succeeds.
2. Watch Stage 2 latency / error rate return to baseline in Grafana
   over 5 minutes.
3. **Re-verify** the red-lane cases that were routed due to fallback.
   These cases now have real BUNEC evidence available; running them
   again moves them to their correct lane. The re-verify command is:
   ```bash
   # Replay verifications that were routed red due to BUNEC fallback,
   # bounded to the outage window:
   curl -sf -X POST -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
     'https://verify.recor.cm/v1/internal/reverify' \
     -d '{"reason":"bunec_recovered","window":{"from":"<outage_start_iso>","to":"<outage_end_iso>"}}'
   ```
   > The `/v1/internal/reverify` endpoint is **TBD — depends on
   > R-VER-1** (no admin endpoint exists today). Until it ships, the
   > equivalent is a SQL-driven enqueue from a one-off script run
   > from a maintenance pod.
4. Watch the queue drain.

### Step 6 — Post-incident

Per [incident-response-template](incident-response-template.md). For
BUNEC-side outages, also coordinate the post-mortem with the BUNEC
operations team to align on root-cause and prevention.

## Verification

The platform is recovered when:

- BUNEC `/healthz` probe (Step 1) returns 200 from a V-engine pod
- Stage 2 latency / error rate are back to baseline in Grafana
- `circuit breaker open` log line no longer appears in the last 5
  minutes of logs
- Cases queued for re-verification during fallback have been
  re-verified
- An incident post-mortem PR is open
- If the outage was credential-rotation-driven, the rotated key is
  verified working AND the old key is revoked on the BUNEC side

## Rollback

If credential rotation was the trigger and the new key is rejected by
BUNEC:

```bash
# Restore the previous Secret content (saved before the patch):
kubectl -n recor apply -f /tmp/recor-bunec-creds-pre-rotation.yaml
kubectl -n recor rollout restart deploy/verification-engine
```

**Always save the prior Secret before patching:**

```bash
kubectl -n recor get secret recor-bunec-creds -o yaml \
  > /tmp/recor-bunec-creds-pre-rotation.yaml
```

If the scale-down (4b) was disproportionate and caused DLQ inundation
on the D → V channel, scale verification-engine back up and follow
[dlq-inundation](dlq-inundation.md) Phase C to replay the
dead-lettered events.

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md)
- [dlq-inundation](dlq-inundation.md)
- [oidc-issuer-outage](oidc-issuer-outage.md)
- [rollback-deployment](rollback-deployment.md)
- [hmac-secret-rotation](hmac-secret-rotation.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [observability-prod-stack](observability-prod-stack.md)
- [incident-response-template](incident-response-template.md)
