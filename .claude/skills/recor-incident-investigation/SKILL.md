---
name: recor-incident-investigation
description: Production incident investigation method. Fires when an incident is being investigated, when a postmortem is being authored, or when the user reports a production anomaly. Loads the investigation method, evidence collection patterns, and report format.
---

# RÉCOR incident investigation

The incident-investigator agent uses this skill; you can use it directly for
authoring postmortems.

## Method

1. **Establish context**: incident identifier, severity, current known impact,
   incident commander (if assigned)
2. **Collect evidence systematically**:
   - Audit log positions during the incident window
   - Service metrics around the incident
   - Distributed traces with error / non-OK status codes
   - Service logs (with PII redaction respected)
   - Recent deployments and configuration changes
   - Any known correlated events
3. **Develop hypotheses**: ranked by evidence support
4. **Test hypotheses**: identify the evidence that would distinguish them
5. **Report**: best-supported hypothesis, alternatives, recommendations

## Evidence sources

- Prometheus / Grafana for metrics
- Tempo for distributed traces
- Loki / OpenSearch for service logs
- Kafka audit topic for audit events
- Argo CD for recent deployments
- Linear / GitHub for recent ticket activity

## Authoring postmortems

Use the PIR template at /docs/runbooks/pir-template.md (and Companion V1 P5).
Quality bar: a future reader who never heard of this incident can understand
what happened, why, what was done, and what changes followed.

## Confidentiality

Postmortems are Restricted by default. Public summaries are derivatives
authored by the communications function. The investigator does not author
public summaries.
