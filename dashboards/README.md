# RÉCOR Grafana dashboards

The platform's Grafana dashboards are committed under the
observability Helm chart (`infrastructure/helm/observability/`)
rather than here, so they ship as part of the same ArgoCD-managed
release as the Prometheus rules.

The two dashboards that follow the platform today:

- `oncall-overview.json` — DLQ size, lane decisions, 5xx rate, OIDC
  outcome counters, Anthropic budget burn rate.
- `audit-chain.json` — declaration_events vs Fabric anchoring delta,
  reconciler pass cadence, divergence counter, peer-SPIFFE-ID denial
  counter.

Closes the audit catalogue's MEDIUM/LOW item flagging the empty
`/dashboards/` directory.
