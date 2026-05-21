# Runbook: chaos-engineering drill (TODO-054)

## When to run

- **Quarterly**: the full suite — pod-kill, network-partition, postgres-pause
  — against the staging cluster, on the first Wednesday of the quarter.
- **Pre-release**: every major release (X.Y.0) runs the pod-kill experiment
  against the release candidate in staging before the prod gate.
- **Post-incident**: after every incident that surfaced an unknown
  failure mode, add a chaos experiment that would have caught it. The
  failure-mode catalogue lives at `docs/security/threat-model.md § known
  failure modes`.

## What we exercise

| Experiment file                                          | What it tests                                                                                |
|----------------------------------------------------------|----------------------------------------------------------------------------------------------|
| `infrastructure/chaos/pod-kill-declaration.yaml`         | Declaration service tolerates pod loss; Service endpoint rerouted; recovery < 30s.           |
| `infrastructure/chaos/network-partition-vengine.yaml`    | Verification engine fail-closes (503) under partition from Postgres + declaration.           |
| `infrastructure/chaos/postgres-pause.yaml`               | Declaration returns 503 (not 500) when Postgres is paused; recovers cleanly post-SIGCONT.    |

## Prerequisites

- Cluster running Litmus 3.x. Install with:
  ```bash
  helm repo add litmuschaos https://litmuschaos.github.io/litmus-helm
  helm upgrade --install litmus litmuschaos/litmus \
       -n litmus --create-namespace
  ```
- Service account `litmus-admin` in namespace `recor` with the
  Litmus-shipped RBAC. See
  `infrastructure/kubernetes/litmus-rbac.yaml` (provisioned by the
  cluster-bootstrap job).
- Grafana annotation token in env `GRAFANA_ANNOTATION_TOKEN` so the
  drill scripts mark the experiment window on the dashboards.

## Procedure

### 1. Pod-kill (declaration)

```bash
kubectl -n recor apply -f infrastructure/chaos/pod-kill-declaration.yaml
kubectl -n recor wait --for=condition=ChaosCompleted=true \
   chaosengine/declaration-pod-kill --timeout=5m
kubectl -n recor get chaosresult declaration-pod-kill-pod-delete \
   -o jsonpath='{.status.experimentStatus.verdict}'
```

Expected verdict: `Pass`.

Acceptance criteria:
- `declaration-readyz` probe: 100% success throughout.
- `declaration-recovery-window` probe: green within 30s of the last kill.
- Service-level error rate during the experiment window: < 1%.

If FAIL: page on-call. The probable cause is one of:
- Insufficient `replicaCount` in the Helm release.
- Missing `PodDisruptionBudget` (see
  `infrastructure/kubernetes/15-pdb.yaml`).
- Slow startup probe — increase `failureThreshold` or shorten the
  ready-gate dependency-check fan-out.

### 2. Network-partition (verification-engine)

```bash
kubectl -n recor apply -f infrastructure/chaos/network-partition-vengine.yaml
kubectl -n recor wait --for=condition=ChaosCompleted=true \
   chaosengine/vengine-network-partition --timeout=5m
```

Expected verdict: `Pass`.

Acceptance criteria:
- `vengine-readyz-failclosed`: 503 throughout the partition window.
  A 200 here is a doctrine D14 violation — the engine is claiming
  ready while it has no Postgres to query.
- `vengine-recovery`: 200 within 30s of partition end.

### 3. Postgres-pause (declaration)

```bash
kubectl -n recor apply -f infrastructure/chaos/postgres-pause.yaml
kubectl -n recor wait --for=condition=ChaosCompleted=true \
   chaosengine/postgres-pause-declaration --timeout=5m
```

Expected verdict: `Pass`.

Acceptance criteria:
- `declaration-submit-fail-closed`: 503 throughout the pause (NOT 500).
  A 500 here is a doctrine D14 violation AND a consumer-contract break.
- `declaration-recovery`: 200 within 30s post-SIGCONT.

## After the drill

1. Export the Litmus ChaosResult CRDs to S3 (the staging cluster's
   chaos archive bucket). Retain for one year.
2. Annotate Grafana with the experiment window. The drill script
   `tools/chaos/annotate-grafana.sh` does this automatically when
   `GRAFANA_ANNOTATION_TOKEN` is set.
3. Update `docs/security/threat-model.md § known failure modes` if
   the drill revealed a new mode.
4. Open a `chaos-drill: <quarter>` issue summarising verdicts +
   action items.

## Doctrines

- **D14 fail-closed** — chaos drills are the load-bearing check
  that the fail-closed contract holds in real failure modes, not
  only in unit tests.
- **D16 observability** — every drill leaves an audit trail
  (ChaosResult CRDs + Grafana annotations + the issue).
