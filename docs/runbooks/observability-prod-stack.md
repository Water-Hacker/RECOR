# Runbook — production observability stack

Production companion to [observability-dev-stack](observability-dev-stack.md).
Where the dev runbook covers the local Docker Compose stack, this one
covers the cluster-deployed stack: the Helm chart at
`infrastructure/helm/observability/` and the ArgoCD application that
reconciles it.

## When this runbook fires

- A service developer reports traces, metrics, or logs not appearing
  in production Grafana
- Grafana itself is unreachable or returns 502 / 503
- Prometheus / Tempo / Loki pods are not Ready
- The ArgoCD `observability` Application reports out-of-sync, degraded,
  or unhealthy
- The on-call cannot see telemetry during another incident — go to
  § "When the observability stack is itself the outage" first
- Alert: `RecorObservabilityComponentDown`

## Prerequisites

- `kubectl` against the production context (`recor-prod`)
- `argocd` CLI authenticated against the production ArgoCD
- `helm` v3.13+ (for diff / template / lint locally; never `helm
  install` direct against prod — ArgoCD reconciles)
- Read access to `s3://recor-prod-grafana/` (the Grafana persistence
  bucket if backed by S3 in your environment; substitute for cloud)
- Knowledge of the chart: see
  `infrastructure/helm/observability/README.md` and
  `infrastructure/helm/observability/Chart.yaml`

## The deployment shape

The stack is **one Helm release** composing four upstream sub-charts
(pinned in `Chart.yaml`):

| Sub-chart | Version | Provides |
|---|---|---|
| `kube-prometheus-stack` | 65.5.1 | Prometheus + Grafana + Alertmanager + node-exporter |
| `tempo-distributed` | 1.21.1 | Tempo (traces) |
| `loki-distributed` | 0.79.3 | Loki (logs) |
| `opentelemetry-collector` | 0.108.0 | OTel collector (gateway) |

Per-environment overlays live at:

- `infrastructure/helm/observability/values-dev.yaml`
- `infrastructure/helm/observability/values-prod.yaml`

Both layer on the base `values.yaml`.

The ArgoCD application is declared at
`infrastructure/argocd/observability.yaml` (release name
`recor-observability`, namespace `observability`).

## When the observability stack is itself the outage

If Grafana is down during another incident, you have **no signal**.
This is the worst time to be debugging. Run this fast-path:

1. Confirm the observability namespace exists and pods are present:
   ```bash
   kubectl -n observability get pods
   ```
2. If pods are absent or terminating, the namespace is being deleted
   (probably an ArgoCD pruning bug). Check:
   ```bash
   kubectl -n observability get all
   argocd app get recor-observability
   ```
3. If ArgoCD itself is down, `kubectl describe` directly. Argo's
   Application CRDs are not load-bearing for the pods running already
   — they reconcile but the pods don't stop because Argo stopped.
4. For the cross-incident: every service exposes `/metrics` directly.
   You can scrape with `kubectl port-forward` and `curl` to confirm
   the service is producing metrics even if Prometheus can't see them:
   ```bash
   kubectl -n recor port-forward deploy/declaration 9090:8080 &
   curl -s localhost:9090/metrics | head -50
   ```
   This is the "drink-from-the-firehose" path; it does not replace
   the dashboard but it tells you the service is alive.
5. Log streams are available via `kubectl logs --since=5m`. Loki is
   a convenience layer on top of those.

Bringing the obs stack back up is the rest of this runbook.

## Procedure

### Step 1 — Identify what's broken

```bash
# Pods first:
kubectl -n observability get pods -o wide

# ArgoCD's view of the Application:
argocd app get recor-observability
# Look at:
# - Sync status:    Synced / OutOfSync
# - Health status:  Healthy / Degraded / Missing
# - Conditions:     any with type=*Warning or *Error
```

Expected healthy state: every pod `Running` (1/1, 2/2, …), all
`Ready`. Sync `Synced`, Health `Healthy`.

Common failure shapes:

- One pod `CrashLoopBackOff` — application-level problem in that pod
- Multiple pods `Pending` — scheduling problem (resources, node taints)
- ArgoCD `OutOfSync` — Helm template change merged but reconcile hasn't
  happened or has failed
- ArgoCD `Degraded` — pods running but failing healthchecks
- `Missing` — namespace or resource was deleted out-of-band

### Step 2 — Per-component diagnostics

#### Prometheus

```bash
# Pod logs:
kubectl -n observability logs sts/prometheus-prometheus-kube-prometheus-prometheus --tail=200

# Disk usage (Prometheus is configured with retention 30d / 50GiB
# per values.yaml; if the PVC fills, scraping pauses):
kubectl -n observability exec sts/prometheus-prometheus-kube-prometheus-prometheus -- \
  df -h /prometheus

# Active targets:
kubectl -n observability port-forward sts/prometheus-prometheus-kube-prometheus-prometheus 9090:9090 &
curl -s localhost:9090/api/v1/targets | jq '.data.activeTargets | map({job, health}) | group_by(.health) | map({(.[0].health): length}) | add'
# Expected: all up.
```

If targets are down, the scrape config is wrong or the target pods
are unhealthy (service-level issue, not obs-stack issue).

If the PVC is full:

```bash
# Confirm:
kubectl -n observability describe pvc prometheus-prometheus-kube-prometheus-prometheus-db-prometheus-prometheus-kube-prometheus-prometheus-0
# Increase via Helm values (values-prod.yaml prometheus.prometheusSpec
# .storageSpec.volumeClaimTemplate.spec.resources.requests.storage),
# then ArgoCD reconciles. The PVC expansion requires the storage class
# to allow expansion (most do).
```

The values override goes through a PR to
`infrastructure/helm/observability/values-prod.yaml`, NOT a
`kubectl edit` against the live PVC. Out-of-band edits drift away
from the Helm-managed state and trigger ArgoCD self-heal.

#### Grafana

```bash
kubectl -n observability logs deploy/recor-observability-grafana --tail=200
```

If 502 from the ingress:

```bash
# Confirm the Grafana pod itself responds (port-forward):
kubectl -n observability port-forward deploy/recor-observability-grafana 3000:3000 &
curl -sI localhost:3000/login | head -3
# 200 OK expected. If yes, the issue is between the pod and the
# ingress — check the Service and the ingress / TLS cert.
```

Admin password is sourced from `Secret/recor-grafana-admin` (per
`values.yaml`: `grafana.admin.existingSecret`). If the Secret is
absent / rotated incorrectly, Grafana starts but logins fail.

```bash
kubectl -n observability get secret recor-grafana-admin -o jsonpath='{.data.admin-password}' | base64 -d ; echo
```

(Do this only on the operator's terminal; do NOT paste the output
into chat or tickets. D18 applies.)

#### Tempo

```bash
# Tempo is multi-component: distributor, ingester, querier, compactor.
kubectl -n observability get pods -l app.kubernetes.io/name=tempo

# Distributor (front door) logs:
kubectl -n observability logs deploy/recor-observability-tempo-distributor --tail=200
```

Common failures:

- Object-storage credentials wrong → ingester / compactor fail to
  write; logs show 403 from S3 / GCS
- Memory pressure on the querier → query timeouts; bump
  `tempo-distributed.querier.resources` in values

#### Loki

```bash
kubectl -n observability get pods -l app.kubernetes.io/name=loki

# Distributor / ingester:
kubectl -n observability logs deploy/recor-observability-loki-distributor --tail=200
kubectl -n observability logs sts/recor-observability-loki-ingester --tail=200
```

Same shape as Tempo for storage-credential failures.

#### OTel collector

```bash
kubectl -n observability get pods -l app.kubernetes.io/name=opentelemetry-collector

kubectl -n observability logs deploy/recor-observability-opentelemetry-collector --tail=200
```

The dev runbook's OTel section
([observability-dev-stack](observability-dev-stack.md) § "OTel
Collector won't start") applies identically in prod with the same
error patterns (YAML parse errors, port conflicts, memory limiter).

### Step 3 — Common operations

#### Force a fresh ArgoCD sync

```bash
argocd app sync recor-observability
```

If sync hangs or fails, inspect:

```bash
argocd app get recor-observability --hard-refresh
argocd app diff recor-observability
```

#### Roll back to a prior Helm chart version

The chart version is pinned in `infrastructure/helm/observability/Chart.yaml`.
Reverting is a code change: PR + merge + ArgoCD reconcile, no
manual `helm rollback`. Use `git revert` of the offending commit, same
as any code rollback.

#### Restart a single component without disturbing others

```bash
kubectl -n observability rollout restart deploy/recor-observability-grafana
# Or for a sub-chart's StatefulSet:
kubectl -n observability rollout restart sts/prometheus-prometheus-kube-prometheus-prometheus
```

Restart is safe — Prometheus persists to PVC, Tempo / Loki persist to
object storage. The 30 s of missing data is acceptable for a restart.

#### Upgrade a sub-chart version

Bumps follow the dependency-upgrade policy in Architecture V3 P7.
Procedure:

1. Update the version in `infrastructure/helm/observability/Chart.yaml`
2. Run `helm dependency update infrastructure/helm/observability/`
   locally (regenerates the `charts/` lock)
3. Mirror any required values changes in `values.yaml` /
   `values-prod.yaml`
4. PR + review + merge — ArgoCD reconciles
5. Watch the rollout from Step 1 + 2 of this runbook

### Step 4 — Self-heal interactions

The ArgoCD application has `syncPolicy.automated.selfHeal: true` (see
`infrastructure/argocd/observability.yaml`). This means out-of-band
edits to live resources are automatically reverted toward the Helm
chart. Two consequences:

1. **Do not edit live resources during an outage** unless you are
   prepared for ArgoCD to undo your edit within minutes. Make the
   change via the Helm chart's values + a PR.
2. **Permitted drift** is documented in the Application's
   `ignoreDifferences` block (HPA-managed replica counts, externally-
   rotated Secrets). Anything else WILL self-heal.

For an emergency manual override, disable self-heal temporarily:

```bash
argocd app set recor-observability --self-heal=false
# … make your manual edit …
# Then re-enable; this is bootstrap-only behaviour:
argocd app set recor-observability --self-heal=true
```

The disable / re-enable bracket must close inside the incident; a
self-heal-disabled Application is a drift accelerator.

### Step 5 — Data retention emergencies

If Prometheus is dropping samples due to disk pressure and the PVC
expansion (Step 2) won't help fast enough:

1. Temporarily drop scrape intervals in values
   (`prometheus.prometheusSpec.scrapeInterval: 60s` instead of 15s)
2. Or temporarily exclude high-cardinality targets via
   `serviceMonitorSelector` in values
3. PR + merge + reconcile

These changes are **bandages**; the root cause (cardinality blow-up,
under-provisioning) is the post-mortem follow-up.

## Verification

The observability stack is healthy when ALL of the following are
true:

- `kubectl -n observability get pods` shows every pod `Running` and
  `Ready`
- `argocd app get recor-observability` shows `Synced` + `Healthy`
- The Grafana UI loads and a known dashboard
  (e.g. `RECOR / Service health`) renders with current data
- Prometheus targets are all `up` (Step 2 query)
- A test trace appears in Tempo within 60 s of being emitted
  (sample: from a V-engine pod,
  `kubectl exec ... -- curl -sf http://localhost:8081/healthz` then
  search Tempo for the trace ID from the response headers)
- A test log line appears in Loki within 30 s
- Alertmanager has no firing alerts on the stack itself

## Rollback

If a Helm values change made things worse:

```bash
# git revert the offending commit on a feature branch:
git -C /path/to/RECOR revert <bad-sha>
# PR through the normal path; merge + ArgoCD reconcile.
```

ArgoCD itself does NOT cache the pre-change desired state across
chart updates — the source of truth is `main`. There is no `argocd
app rollback` for a Helm chart change; the rollback IS the revert PR.

For pure operational scaling (replica counts, PVC size) where the
change is in-cluster only, scale back via `kubectl edit deploy/…` and
remove the drift from the values file in the same revert PR.

## Related runbooks

- [observability-dev-stack](observability-dev-stack.md) — the
  developer-facing Docker Compose stack covered by the same patterns
- [oncall-triage-tree](oncall-triage-tree.md) — the entry point that
  brings you here when telemetry is missing
- [deploy-new-version](deploy-new-version.md) — for the obs chart
  upgrade path
- [rollback-deployment](rollback-deployment.md) — for reverting a bad
  observability change
- [incident-response-template](incident-response-template.md)
