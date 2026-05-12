# ADR 0001: Observability stack — dev compose + production Helm

Date: 2026-05-11
Status: Accepted
Authors: SRE lead, lead architect
Reviewers: Security lead, Technical Advisory Function

## Context

Architecture V5 P22 commits to OpenTelemetry-driven observability with
Prometheus (metrics), Tempo (traces), Loki (logs), and Grafana
(dashboards), and identifies kube-prometheus-stack, tempo-distributed,
loki-distributed, and opentelemetry-collector as the upstream charts.
F-007 (Companion V7 P30) materialises this commitment in Sprint 1.

Two deployment surfaces need to exist:

1. A **production-grade Kubernetes deployment** for the consortium's
   operational clusters, sized for HA and the 30-day retention commitment.
2. A **developer-laptop dev surface** that every engineer can bring up in
   under five minutes without provisioning a Kubernetes cluster.

The choice between surfaces is not exclusive — the question is how they
relate.

## Decision

Ship both, sharing image versions and conceptual topology.

- **Production:** Helm chart at `infrastructure/helm/observability/`
  composing four upstream sub-charts via Helm `dependencies:`. Two
  environment overlays (`values-dev.yaml`, `values-prod.yaml`) layered on
  a shared `values.yaml`. Deployed by ArgoCD
  (`infrastructure/argocd/observability.yaml`).
- **Dev:** Docker Compose stack at `infrastructure/observability-dev/`
  running the same five components as single containers. Same image tags
  as the production sub-charts. Runnable as
  `cd infrastructure/observability-dev && docker compose up -d`.

Engineers iterate against the dev compose stack day-to-day; the
production cluster runs the Helm chart.

## Considered alternatives

### Alternative A: Helm-only (no dev compose)

Considered but not chosen. Without a dev compose stack, every engineer
needs a local Kubernetes cluster (kind / k3s / minikube) plus
helm-installed components. This is a heavyweight prerequisite for routine
development. Cost: every new engineer's bootstrap takes hours; the smoke
loop is slow; CI cannot exercise the stack in the runner without
provisioning kind.

### Alternative B: Bespoke Helm templates (no sub-charts)

Considered but not chosen. Writing custom Helm templates for Prometheus,
Tempo, Loki, Grafana, and OTel Collector duplicates work the upstream
projects already do well. The upstream sub-charts are widely deployed and
get security patches faster than a bespoke fork would. The cost is a
shallow Helm hierarchy and slightly more YAML in `values.yaml`; the
benefit is upstream maintenance leverage.

### Alternative C: Grafana Cloud (managed)

Considered but not chosen. Grafana Cloud is operationally easy but the
data leaves the consortium's perimeter. For a sovereign infrastructure
project (Architecture V1 § Sovereignty by construction), observability
data — which includes service logs that may contain PII — must reside
within consortium-controlled infrastructure. Grafana Cloud is the right
answer for many SaaS projects; it is the wrong answer here.

### Alternative D: AWS Distro for OpenTelemetry (ADOT)

Considered but not chosen. ADOT is well-engineered but couples the stack
to AWS-managed services for metrics (CloudWatch / AMP) and traces
(X-Ray). The platform deploys across Yaoundé and Douala (potentially on
non-AWS infrastructure), and the consortium must retain the option to
move clouds. Open-source stack chosen for portability.

### Alternative E: New Relic / Datadog / observability vendor

Considered but not chosen. Same sovereignty reasoning as Alternative C,
plus per-data-point licensing cost is prohibitive at the platform's
projected event volume (~100 K declarations / year × 9-stage pipeline =
~900 K span sets / year just for verification).

## Consequences

### Easier

- Engineers iterate locally without a Kubernetes cluster
- Dev and prod share image versions; bumps are coordinated
- Upstream sub-charts receive security patches without bespoke maintenance
- ArgoCD reconciles drift; we don't operate kubectl-apply pipelines
- The smoke test pattern (compose-up → telemetrygen → query Tempo) is
  reusable for future service-specific smoke tests

### Harder

- Two deployment surfaces to keep in sync — drift risk between
  `docker-compose.yaml` and `values.yaml`. Mitigation: ADR-mandated
  mirror discipline; PR review for both surfaces; future ticket can add
  a "drift check" CI step.
- Helm sub-chart values keys are upstream-defined; learning curve for
  contributors who haven't deployed the upstream charts before.
- Image-tag-pinning (vs. digest-pinning) leaves a small supply-chain
  surface. Follow-up: bump to digest-pinning once production deployment
  has shown which digests are stable, per V3 P7 dependency upgrade
  governance.

### New commitments

- Engineers maintain the Grafana dashboard JSON in both surfaces
- SRE owns ArgoCD-managed prod reconciliation
- The 24/7 observability-smoke schedule on `main` catches upstream
  image-tag drift early

### Old commitments now obsolete

None — this is a Sprint 1 foundational decision; no prior commitments
to displace.

## Doctrines applied

- **D01** — completeness: dev + prod + smoke + runbook + ADR shipped
  together.
- **D14** — fail-closed: smoke test fail-closes when any component is
  unhealthy; missing required-check holds merges.
- **D16** — observability is non-optional: this ticket *is* the
  observability foundation; every subsequent service depends on the
  sinks existing.
- **D17** — zero trust: dev stack binds to 127.0.0.1 only; production
  uses mTLS between collector and backends.
- **D18** — no secrets: dev admin password lives in gitignored `.env`;
  prod admin password lives in ExternalSecret rotated from Vault.
- **D20** — supply chain: image tags pinned; digest pinning is the
  follow-up improvement.

## Document references

- Architecture V5 P22 — Observability cross-cutting concern
- Architecture V3 P7 — Dependency policy (image version upgrades)
- Companion V7 P30 — F-007 ticket definition

## Implementation

- Status: Implemented in PR (this commit)
- Sprint: PI-1 Sprint 1
- Linked tickets: F-007
- Follow-up tickets:
  - Promote `observability-smoke` to required-check after a week of green
    runs (separate PR; updates `tools/ci/apply-branch-protection.sh`)
  - Migrate image tags to digests once production deployment confirms
    stable digests
  - Production runbook authoring (separate ticket once the consortium's
    Kubernetes cluster is provisioned)
