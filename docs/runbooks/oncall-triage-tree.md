# Runbook — on-call triage tree

The page-in-the-middle-of-the-night entry point. If you are not sure what
is broken, start here. This runbook does not fix anything; it routes you
to the runbook that does.

## Trigger

You have been paged or you have an incoming user / consumer / operator
report of "something is wrong with RÉCOR." You do not yet know which
component is implicated.

## Prerequisites

- `gh` CLI authenticated against `Water-Hacker/RECOR` (or the consortium
  fork) with at least `read:org`
- `kubectl` configured against the production cluster context
  (`recor-prod`) — see § "Cluster access" below
- Browser access to the Grafana production tenant
  (URL is published in the on-call rotation handbook; not committed to
  this repo per D18)
- Read access to the `#oncall-recor` channel
- Your on-call laptop has a recent (≤ 24 h old) `kubectl auth can-i`
  refresh; if you have not used the cluster in a week, run
  `kubectl auth can-i list pods -n recor` first to confirm

### Cluster access

Production cluster context is provisioned via the on-call bootstrap. If
`kubectl config get-contexts | grep recor-prod` returns nothing, your
on-call enrolment is incomplete — page the SRE lead and use the
backup on-call until access is restored.

## Procedure

### Step 0 — Acknowledge the page

Inside 5 minutes of being paged:

1. Acknowledge in PagerDuty (or whatever pager surface is active).
2. Post in `#oncall-recor`: "On it, investigating, ETA on first
   update: 15 min." Even if you have nothing yet, the visible
   acknowledgement matters.

The acknowledgement starts the incident clock. Every later runbook
assumes you have done this.

### Step 1 — Identify the symptom class

Answer this question first: **what is the user-visible failure mode?**

| Symptom class | Indicator | Jump to |
|---|---|---|
| Declarations fail to submit (5xx from `/v1/declarations`) | Portal users see "submission failed" toast; declaration service error rate > 1 % | § A. Declaration submit failures |
| Verifications stuck (lane = pending for > 60 s) | V-engine queue depth growing; case_id never resolves | § B. Verification engine stall |
| Cross-service replication broken (D ↔ V) | `outbox.dispatched_at IS NULL` rows accumulating | § C. Outbox / DLQ |
| Authentication failing (401 on every authenticated route) | All requests blocked, including healthy clients | § D. Auth outage |
| Receipts / case lookups returning 500 | Read-side broken; submission may still work | § E. Read-path failure |
| Whole service unreachable (502 / 503 from ingress) | Healthz failing at ingress; pods crashlooping or unreached | § F. Service down |
| Background jobs idle (DLQ growing, no replay activity) | DLQ size > 100 and not draining | [dlq-inundation](dlq-inundation.md) |
| Observability stack itself broken | Grafana 502; Prometheus not scraping; you have no signal | [observability-prod-stack](observability-prod-stack.md) § "When the observability stack is itself the outage" |

If two or more symptom classes are firing simultaneously, work the
**downstream-first** rule: the symptom closest to the user is the one
you announce; the upstream cause is the one you fix. Both go in the
incident timeline (see [incident-response-template](incident-response-template.md)).

### Step 2 — Confirm the symptom with telemetry

Do not trust a single report. Open Grafana → `RECOR / Service health`
dashboard and confirm the symptom on at least one of:

- Request rate / error rate / latency panel for the implicated service
- DLQ depth panel (`recor_outbox_dlq_size`)
- Lane outcome distribution (`recor_verification_lane_total`)

If telemetry contradicts the report, the report may be a single-user
issue, not a service incident. Reply to the reporter asking for case_id
/ correlation_id and time-of-failure before paging further.

If Grafana itself is unreachable, jump to
[observability-prod-stack](observability-prod-stack.md) — you cannot
triage without signal.

### Step 3 — Check recent changes

Most production incidents are caused by the most recent deploy. Before
anything else:

```bash
gh run list --workflow=publish-images.yaml --limit 5
```

> Note: `publish-images.yaml` is delivered by **CI-4** (see
> `docs/PRODUCTION-TODO.md`). Until that workflow ships, substitute
> `gh run list --limit 10` and visually scan for any build/deploy
> workflow that ran in the last 4 hours.

Cross-reference the most recent deploy timestamp with the symptom-onset
timestamp. If the symptom started within 30 minutes of a deploy:

→ Strongly suspect the deploy. Go to
[rollback-deployment](rollback-deployment.md) immediately. Do NOT spend
time on deeper diagnostics; the rollback is fast and reversible.

If the symptom started > 1 hour before the most recent deploy, the
deploy is unlikely to be the cause — continue triage.

### Step 4 — Route to the specific runbook

Use the symptom class table from Step 1 to pick the next runbook.

#### § A. Declaration submit failures

1. Check authentication first (Step D's first command). If auth is
   healthy but submits still fail, suspect database / outbox.
2. Run:
   ```bash
   kubectl -n recor logs deploy/declaration --tail=200 | grep -E "ERROR|WARN"
   ```
3. Common causes:
   - **DB connection pool exhausted** → check `recor_pg_pool_size`;
     fix by scaling replicas (transient) or investigating slow queries
     (root cause).
   - **Outbox table lock contention** → `pg_stat_activity` will show
     long-held locks on `outbox`; see [dlq-inundation](dlq-inundation.md).
   - **OIDC verifier failure** → see § D.

#### § B. Verification engine stall

1. Check stage-level latency in Grafana → `RECOR / Verification stages`.
   The stalled stage is the one with rising p99.
2. If Stage 2 (identity / BUNEC), jump to
   [bunec-adapter-outage](bunec-adapter-outage.md).
3. If Stage 1 (schema), the input is malformed — look at the failing
   case_id; this is rarely a platform incident, more often a misconfigured
   client.
4. If multiple stages are slow simultaneously, suspect DB or upstream
   D→V replication; see § C.

#### § C. Outbox / DLQ

Go directly to [dlq-inundation](dlq-inundation.md). That runbook
covers both the "DLQ growing" and "outbox not draining" cases.

#### § D. Auth outage

Run the OIDC discovery probe:

```bash
kubectl -n recor exec deploy/declaration -- \
  curl -sf "${OIDC_ISSUER_URL}/.well-known/openid-configuration" \
  | head -20
```

If this fails or hangs, the IdP itself is down. Go to
[oidc-issuer-outage](oidc-issuer-outage.md).

If the discovery doc returns but the JWKS endpoint is empty / wrong:

```bash
kubectl -n recor exec deploy/declaration -- \
  curl -sf "${OIDC_ISSUER_URL}/$(curl -sf ${OIDC_ISSUER_URL}/.well-known/openid-configuration | jq -r .jwks_uri)"
```

If JWKS is bad, the IdP is misconfigured; treat as
[oidc-issuer-outage](oidc-issuer-outage.md).

#### § E. Read-path failure

1. `kubectl -n recor logs deploy/declaration --tail=100 | grep "GET /v1"`
2. Confirm Postgres read replica is healthy:
   `kubectl -n recor exec sts/postgres-declaration-replica-0 -- pg_isready`
3. If the replica is lagging > 30 s, queries hit it and return stale
   data; failover or fall back to the primary by setting
   `DATABASE_READ_URL=$DATABASE_URL` in the deployment env, then
   restart the deployment.

#### § F. Service down

1. `kubectl -n recor get pods -l app=declaration` (or `verification-engine`,
   `declarant-portal`)
2. If pods are in `CrashLoopBackOff`, the last deploy is bad → go to
   [rollback-deployment](rollback-deployment.md).
3. If pods are `Running` but unreached, check the Service / Ingress:
   ```bash
   kubectl -n recor get svc,ingress
   kubectl -n recor describe ingress recor-declaration
   ```
4. If TLS-cert expiry is in the past: the cert-manager renewal failed.
   This is a sub-incident; document in your timeline and open a ticket
   against `@recor/security-team`.

### Step 5 — Post the first update

By 15 minutes after the page, post in `#oncall-recor`:

```
Update — <symptom class from Step 1>
Confirmed: <which dashboards confirm>
Suspected cause: <best current guess>
Next runbook: <runbook you're heading into>
ETA next update: <15-30 min>
```

If you do not have an ETA, say so explicitly: "Cause unclear, will
update when I narrow down." Silence breeds escalation. The visible
update is the substitute for resolution.

### Step 6 — Hand off if you are stuck

If 45 minutes have passed and the symptom is still active and you do
not have a clear path forward:

1. Page the secondary on-call (PagerDuty escalation).
2. Hand off in a single message that names the symptom, the
   runbooks you have already worked, and the open questions.
3. Stay on the bridge / channel until the secondary explicitly takes
   the incident.

D08 (no dangling threads) applies to incidents too: do not silently
abandon an investigation.

## Verification

You know you are out of "triage" mode and into "fix" mode when:

- You have identified the implicated component (named in Step 1)
- You have identified the change (deploy, config, upstream outage) that
  caused it (or have explicitly noted "no recent change identified;
  treating as latent defect")
- You have a runbook open for the fix
- You have posted the first incident update

## Rollback

There is nothing to roll back in this runbook — it is read-only triage.
If you executed a destructive step from a downstream runbook (e.g.
restarted a deployment) and made things worse, the downstream
runbook's "Rollback" section applies.

## Related runbooks

- [rollback-deployment](rollback-deployment.md)
- [dlq-inundation](dlq-inundation.md)
- [oidc-issuer-outage](oidc-issuer-outage.md)
- [bunec-adapter-outage](bunec-adapter-outage.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [observability-prod-stack](observability-prod-stack.md)
- [incident-response-template](incident-response-template.md)
