# recor-observability

Helm chart that composes the platform's observability stack:

- **kube-prometheus-stack** — Prometheus, Alertmanager, Grafana, node-exporter, kube-state-metrics
- **tempo-distributed** — distributed tracing backend
- **loki-distributed** — log aggregation
- **opentelemetry-collector** — receives OTLP from services, fans out to the three backends

Architecture reference: V5 P22.

## Usage

```bash
# Dependencies
helm dependency update infrastructure/helm/observability

# Lint
helm lint infrastructure/helm/observability \
  -f infrastructure/helm/observability/values-dev.yaml

# Dev cluster
helm upgrade --install --create-namespace -n observability \
  recor-observability \
  infrastructure/helm/observability \
  -f infrastructure/helm/observability/values-dev.yaml

# Production
helm upgrade --install --create-namespace -n observability \
  recor-observability \
  infrastructure/helm/observability \
  -f infrastructure/helm/observability/values-prod.yaml
```

Production deployment is normally driven by ArgoCD (`infrastructure/argocd/observability.yaml`), not by direct `helm upgrade`.

## Environment overlays

| File | Use |
|---|---|
| `values.yaml` | Base values applied in all environments |
| `values-dev.yaml` | Dev k8s overrides: single replica, in-cluster MinIO storage, ephemeral retention |
| `values-prod.yaml` | Production overrides: HA replicas, PVC sizes, external S3, ExternalSecrets, anti-affinity |

## What this chart is NOT

This chart is **not** the dev surface engineers use day-to-day. The dev surface is the docker-compose stack at `infrastructure/observability-dev/`, which is runnable on any developer laptop without a Kubernetes cluster.

The Helm chart is the production deployment artefact and the in-cluster dev artefact. The docker-compose stack and the Helm chart deploy the same set of components but at different scales and through different orchestrators.

## Verification post-deploy

```bash
# Wait for all pods ready
kubectl -n observability rollout status statefulset -l app.kubernetes.io/instance=recor-observability --timeout=10m

# Grafana port-forward + health
kubectl -n observability port-forward svc/recor-observability-grafana 3000:80 &
curl -fsS http://localhost:3000/api/health

# Verify trace ingestion
# (run telemetrygen against the collector; expect traces in Tempo)
```

Detailed operational procedures are in `docs/runbooks/observability-dev-stack.md` (dev stack) and the upcoming `docs/runbooks/observability-prod-stack.md` (production, future ticket).

## Modification

CODEOWNERS routes changes here to `@recor/sre-team`. Sub-chart version bumps are governed under the dependency-upgrade policy in Architecture V3 P7.
