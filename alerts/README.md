# RÉCOR Prometheus alert rules

Closes the MEDIUM/LOW catalogue items for missing Prometheus
alert-rule enforcement on:

- DLQ inundation
- Anthropic budget exhaustion
- OIDC outage
- HMAC replay-window refusals (FIND-012)
- SPIFFE peer-ID denials (FIND-017)
- Audit-chain divergence (FIND-016)

## Apply

Mount via the observability Helm chart's `additionalPrometheusRules`
value, or via Prometheus' `rule_files:` directive when running stand-
alone:

```yaml
rule_files:
  - /etc/prometheus/rules/recor-prometheus-rules.yaml
```

## Doctrines

- **D16 observability** — every load-bearing metric we ship has an
  alert here. Missing alerts mean an operator finds out a regression
  from a user, not from oncall.
- **D14 fail-closed** — alert severities are page / ticket; no
  warning-only tier that gets silenced.
