# Runbook — Observability dashboards & alerts (OBS-1)

This runbook documents the four Grafana dashboards and three Prometheus
alerts that OBS-1 introduces. The dashboards live under
`infrastructure/observability-dev/grafana/dashboards/` and are
auto-provisioned into the dev Grafana at startup; the alerts live in
`infrastructure/observability-dev/alert-rules.yaml` and are loaded by
`prometheus.yml`'s `rule_files:` directive.

The on-call rotation should treat this runbook as the single entry
point when any of the three OBS-1 alerts page or fire. Each alert
section below names the dashboard panel that contains the relevant
context, the immediate triage steps, and the rollback / mitigation
options.

## Audience

- On-call engineers responding to alerts.
- Operators investigating user-reported issues.
- Reviewers verifying that a new metric is correctly wired (the
  metric inventory below should stay in sync with the
  `services/<name>/src/metrics.rs` modules).

## Doctrine compliance summary

- D14 fail-closed: `/metrics` cannot crash the service even when the
  encoder fails; the handler returns HTTP 500 and the service keeps
  serving real traffic.
- D17 zero-trust: `/metrics` ships no authentication. The deployment
  expectation is in-cluster network only — never internet-facing. The
  production Helm chart MUST enforce this via NetworkPolicy; see
  the "Deferred for Helm" section below.
- D18 no secrets / no high-cardinality labels: every metric label is
  a bounded enum (`lane`, `result`, `kind`, `probe`, `subscriber`,
  matched-path templates). No principals, no UUIDs, no free strings.

## Deployment expectation

The Prometheus `/metrics` endpoint is operational and must not be
exposed to the public internet. The expected deployment shape:

- Production: services run inside the cluster; `/metrics` is reachable
  only from in-cluster Prometheus pods. Ingress rules MUST exclude the
  `/metrics` path. The cluster's NetworkPolicy SHOULD restrict ingress
  on the metrics port to the Prometheus namespace.
- Dev: services bind on localhost only by default. The dev observability
  stack (`infrastructure/observability-dev/`) scrapes via
  `host.docker.internal`. Operators running both stacks concurrently
  must NOT publish the service ports on the LAN.

## Metric inventory

The complete metric set emitted by each service. Names must be unique
across services; label values must remain bounded.

### Shared (both services)

- `http_requests_total{method,path,status}` — counter
- `http_request_duration_seconds_bucket{method,path,le}` — histogram
- `recor_oidc_jwks_fetch_latency_seconds_bucket{result,le}` — histogram
- `recor_oidc_verify_total{result}` — counter,
  `result ∈ {success, invalid, unavailable}`
- `recor_health_check_duration_seconds_bucket{probe,le}` — histogram,
  `probe ∈ {healthz, readyz}`
- `recor_outbox_dlq_size` — gauge
- `recor_outbox_dlq_replays_total{result}` — counter,
  `result ∈ {success, failure}`

### Declaration only

- `recor_declarations_submitted_total{kind}` — counter,
  `kind` is the bounded `DeclarationKind` enum
- `recor_declarations_amended_total{result}` — counter
- `recor_declarations_corrected_total{result}` — counter
- `recor_outbox_undispatched` — gauge
- `recor_relay_delivery_latency_seconds_bucket{subscriber,le}` —
  histogram

### Verification engine only

- `recor_verification_cases_total{lane}` — counter,
  `lane ∈ {green, yellow, red}`
- `recor_fusion_belief_true_bucket{lane,le}` — histogram
- `recor_fusion_belief_false_bucket{lane,le}` — histogram

When you add a new metric, update this inventory in the same PR. The
PR template's "observability" checkbox depends on it.

## Dashboards

The four dashboards together cover the operational surface for OBS-1.
Each dashboard's purpose, panels, and recommended drill-down paths
follow.

### `platform-health.json` — RÉCOR Platform Health

**Purpose.** First-look dashboard. Answers "is the platform serving
traffic right now?" in 30 seconds. Open this when a paging alert
fires before drilling into a service-specific dashboard.

**Panels.**

1. Service up status — per-service `up{job=~"recor-.*"}`. Red ⇒ scrape
   target is down (service crashed, port closed, or Prometheus cannot
   reach `host.docker.internal`).
2. Request rate by service — 1m smoothed `rate(http_requests_total)`
   per service.
3. 5xx error ratio — `5xx/all` per service. Crosses red at 5% — same
   threshold as the `RecorHttp5xxRateHigh` alert.
4. p99 HTTP latency by service.
5. p99 latency by endpoint — same data, broken down by matched-path
   template. Identify slow endpoints.
6. Declaration submit rate by kind — domain-side traffic profile.

**Recommended drill-down.** If the 5xx panel turns red, jump to the
relay-health or verification-health dashboard depending on which job
is implicated, then to Tempo via the per-request `trace_id` exemplar.

### `relay-health.json` — RÉCOR Relay Health

**Purpose.** Watch the outbox-relay subsystem. Open this when the
`RecorDlqOversized` or `RecorRelayLatencyHigh` alert fires, or when a
consumer reports missing events.

**Panels.**

1. Outbox undispatched (gauge) — current count.
2. DLQ size (gauge) — current count, with the same red threshold (100)
   as the alert.
3. DLQ replay rate — by `result`. Operator activity visualised.
4. Relay delivery latency — p50/p95/p99 per subscriber.
5. Outbox undispatched over time.
6. DLQ size over time.

**Recommended drill-down.** When the DLQ size is climbing, list the
DLQ via `GET /v1/internal/outbox-dlq` (declaration) or
`/v1/internal/verification-outbox-dlq` (verification engine) using an
admin principal, inspect `last_error` on the rows, fix the upstream
issue, then `POST .../replay` the rows. See
`docs/runbooks/dlq-inundation.md` for the full sequence.

### `verification-health.json` — RÉCOR Verification Health

**Purpose.** Watch the verification engine's lane router and fusion
output distribution. Open this to investigate "why did my declaration
go yellow / red?" and to monitor drift in the lane distribution.

**Panels.**

1. Verification cases per lane — colour-coded by lane.
2. Lane share (percent stacked).
3. Fused authenticity belief (true) — p50/p95/p99 per lane.
4. Fused authenticity belief (false) — p50/p95/p99 per lane.
5. Submit-verification p99 latency — wall-clock cost of the pipeline.

**Recommended drill-down.** Drift in the lane share is a higher-order
signal: pipeline regression, BUNEC adapter degradation, or a shift in
declarant traffic. Correlate against `recor_declarations_submitted_total`
on the platform-health dashboard and the BUNEC adapter span on Tempo.

### `auth-health.json` — RÉCOR Auth Health

**Purpose.** OIDC authentication subsystem. Open this when the
`RecorOidcVerifierDown` alert fires or when declarants report being
unable to authenticate.

**Panels.**

1. OIDC verify outcomes — `success` / `invalid` / `unavailable` rates.
2. OIDC unavailable rate (alert source) — stat panel cued to the
   alert threshold (0.5/s, red at the breach).
3. JWKS fetch latency — p50/p95/p99 by outcome.
4. HTTP 401 rate by service.

**Recommended drill-down.** If the `unavailable` series turns red,
follow `docs/runbooks/oidc-issuer-outage.md`. If the `invalid` series
spikes but `unavailable` is flat, look for a recently-rotated key in
the upstream IdP that hasn't propagated to the platform's caches yet.

## Alerts

The OBS-1 alert set. Each rule annotates `runbook_url` pointing back
to this section.

### Alert: RecorDlqOversized

**Trigger.** `recor_outbox_dlq_size > 100 for 10m`. Severity: page.

**What is happening.** Either service's DLQ has accumulated more than
100 rows and stayed there for ten minutes. This is the platform's
fail-closed boundary for cross-service replication — rows reach the
DLQ only after exhausting dispatch attempts.

**Procedure.**

1. Open the relay-health dashboard and confirm the DLQ size matches the
   alert. If the gauge is now < 100, the alert is recovering — wait
   for it to clear and capture a post-mortem.
2. Identify which service via the `service` label on the gauge.
3. List the DLQ via the admin endpoint (see `dlq-inundation.md` for
   the auth + payload shape).
4. Inspect a sample of `last_error` strings. Classify:
   - `transport: <error>` ⇒ subscriber unreachable or slow; check the
     subscriber's `up` status and 5xx rate.
   - `http 4xx: ...` ⇒ payload incompatibility (schema mismatch, bad
     HMAC); inspect the subscriber's logs for the rejection cause.
   - `http 5xx: ...` ⇒ subscriber crashed mid-processing.
5. Fix the upstream cause first.
6. Replay the DLQ rows once the cause is fixed. Replay is atomic per
   row; partial-batch replay is safe.

### Alert: RecorRelayLatencyHigh

**Trigger.** `histogram_quantile(0.99, rate(recor_relay_delivery_latency_seconds_bucket[5m])) > 30 for 5m`.
Severity: warn.

**What is happening.** The 99th-percentile relay delivery latency has
exceeded 30 seconds for five minutes. Healthy delivery is sub-second.
Sustained latency indicates either a slow subscriber, a network path
issue, or the subscriber is occasionally timing out (the relay's
default reqwest timeout is 10s; latency over that becomes a transport
error which dead-letters after the retry limit).

**Procedure.**

1. Open relay-health → "Relay delivery latency" panel and identify
   which subscriber is slow.
2. Check the subscriber's own latency dashboard.
3. Run `curl -v` from the relay's environment to the subscriber URL to
   isolate the network path.
4. If the subscriber is the verification engine and only one path is
   slow, see `verification-health.json` → "Submit-verification p99
   latency" to determine whether the pipeline itself has regressed.

### Alert: RecorOidcVerifierDown

**Trigger.** `rate(recor_oidc_verify_total{result="unavailable"}[5m]) > 0.5 for 5m`.
Severity: page.

**What is happening.** The OIDC verifier is returning the
`unavailable` outcome more than 0.5 times per second. This is the
infrastructure-fault branch — JWKS or discovery endpoint unreachable.
Every protected REST + gRPC call is failing closed (D14): bearer
authentication returns HTTP 500 ("oidc discovery failed") rather than
allowing through. Declarant traffic will back up.

**Procedure.**

1. Open auth-health → "OIDC verify outcomes" to confirm the spike is
   on `unavailable`, not `invalid`. (If it's `invalid`, follow the
   bad-token branch in `oidc-issuer-outage.md`.)
2. Check the configured `OIDC_ISSUER_URL` discovery endpoint health
   from the cluster (use a debug pod with `curl`).
3. If the issuer is genuinely down, follow
   `docs/runbooks/oidc-issuer-outage.md` for the failover sequence.
4. If the issuer is healthy from outside the cluster but unreachable
   from inside, suspect a NetworkPolicy or DNS regression.

### Alert: RecorHttp5xxRateHigh (operational follow-on)

**Trigger.** 5% of HTTP responses are 5xx for 10m. Severity: warn.

**Procedure.** Open platform-health → "5xx error ratio" + "p99 latency
by endpoint", identify the implicated endpoint, jump to Tempo via the
trace exemplar for a recent error sample.

## Adding a new metric

The metric module is the single registration point. New metrics
follow the same path:

1. Declare the collector in `crate::metrics::Metrics::new()`.
2. Use a bounded label set — enum or matched-path template only. D18
   forbids principal, UUID, or free-string labels.
3. Add it to the metric inventory in this runbook.
4. Add a panel to the relevant dashboard. If no dashboard fits, that
   is a signal the new metric is misaligned to operator concerns; ask
   why before adding it.
5. If the new metric drives an alert, add the rule to
   `alert-rules.yaml` and reference the runbook section above.

## Deferred for Helm

The dev observability stack provisions Grafana dashboards via a
mounted directory. Production deployment uses the
`infrastructure/helm/observability/` chart; the four new dashboards
need to be made available to the production Grafana instance as
ConfigMaps mounted into the dashboard sidecar (the standard Grafana
`grafana_dashboard: "1"` annotation pattern).

This work is non-trivial because:

- Each dashboard JSON is ~5-10 KB; four ConfigMaps OR a single
  ConfigMap with four keys.
- The Helm chart's `values-prod.yaml` must reference whichever shape
  ships.
- The provisioning must respect the production folder convention
  ("RÉCOR Production" vs the dev stack's "RÉCOR Dev" folder).

Deferred to follow-up ticket
**OBS-1-FOLLOWUP-HELM** — to be filed once OBS-1 merges. The dev
stack works without this; the alerts and metrics are first-class
regardless. The followup is purely about prod-Grafana provisioning.

## Smoke check

After merging OBS-1 the on-call rotation should run a smoke locally:

```bash
# Bring up the integration stack (services + observability)
just observability-up
just integration-up

# Hit /metrics on both services
curl -s http://localhost:8080/metrics | head -40
curl -s http://localhost:8081/metrics | head -40

# Expect Prometheus exposition format with the recor_* names.

# Open the dashboards
open http://localhost:3000/d/recor-platform-health
open http://localhost:3000/d/recor-relay-health
open http://localhost:3000/d/recor-verification-health
open http://localhost:3000/d/recor-auth-health
```

If any panel reads "No data" while traffic is flowing, the most
common causes are (a) the scrape target points at the wrong host —
verify `host.docker.internal` resolves from inside the Prometheus
container — and (b) the metric was renamed without updating the
dashboard query.
