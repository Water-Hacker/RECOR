# RÉCOR NetworkPolicies

Closes audit FIND-007 (`/metrics` endpoint reachable from outside the
cluster) and seeds the network-segmentation layer the audit flagged
as missing under FIND-008.

## Posture

The policies in this directory ship the **default-deny** baseline the
audit's threat-model assumes:

1. **`00-default-deny.yaml`** — every pod in the `recor` namespace
   starts with ingress AND egress denied. Subsequent policies
   selectively allow traffic.
2. **`10-allow-dns.yaml`** — allow egress to kube-dns so service
   discovery keeps working.
3. **`20-allow-business-ports.yaml`** — allow ingress on the
   business ports (declaration `8080`, verification-engine `8081`,
   person-service `8082`, entity-service `8083`) from the platform
   ingress controller AND from sibling RÉCOR pods.
4. **`30-allow-metrics-scrape.yaml`** — allow ingress on the
   **separate metrics ports** (`9080-9083`) ONLY from pods bearing
   the Prometheus scraper's labels. This is the FIND-007 closure
   path: the application binds `/metrics` on the metrics port via
   the new `METRICS_BIND_ADDR` env var, this policy makes that port
   reachable only by the Prometheus scraper.

## How it composes with the application change

The FIND-007 closure has TWO halves:

- **Application:** every service exposes a `METRICS_BIND_ADDR`
  config (env var). When set (e.g. `0.0.0.0:9081` for V-engine),
  `/metrics` is bound on that separate listener and **removed from
  the main business listener**. The main listener no longer carries
  any /metrics route — Trivy-class fingerprint surfaces (DLQ size,
  Anthropic budget, governor rejection rate) cannot leak via the
  ingress.
- **Network:** the policy in `30-allow-metrics-scrape.yaml`
  restricts ingress on the metrics port to the Prometheus pod
  CIDR. Even an attacker with intra-cluster lateral movement
  cannot scrape it from a non-scraper pod.

Operators MUST set `METRICS_BIND_ADDR` in any deployment where the
main listener is reachable from outside the cluster. The
application defaults to empty (single-port dev posture) so existing
single-port deployments and integration tests keep working.

## Labels & selectors

The policies assume the following labels on RÉCOR pods (set by the
Helm charts under `infrastructure/helm/`):

| Label | Values |
|---|---|
| `app.kubernetes.io/part-of` | `recor` |
| `app.kubernetes.io/component` | `declaration` / `verification-engine` / `person-service` / `entity-service` |
| `recor.cm/role` | `service` (any RÉCOR backend pod) |

The Prometheus scraper is expected to carry:

| Label | Value |
|---|---|
| `app.kubernetes.io/name` | `prometheus` |
| `app.kubernetes.io/component` | `metrics-scraper` |

Adjust the selectors to match your Helm chart's actual labels before
applying. The default values match the planned `infrastructure/helm/`
chart structure.

## How to apply

```
kubectl apply -f infrastructure/networks/ -n recor
```

ArgoCD reconciles drift automatically once
`infrastructure/argocd/networks.yaml` is wired up (separate ticket
when `infrastructure/argocd/` gains a `networks` application).

## Audit cross-ref

- **FIND-007** (HIGH): closed by the combination of `METRICS_BIND_ADDR`
  + `30-allow-metrics-scrape.yaml`.
- **FIND-008** (HIGH): partially closed — this directory is no
  longer empty. Full closure of FIND-008 requires the Helm charts,
  Terraform, and OPA policy stack to land alongside (multi-week
  workstream).
