# RÉCOR — dev observability stack

Local docker-compose deployment of the platform's observability backends for
day-to-day engineering. Implements the dev-target half of F-007.

The production target is the Helm chart at `infrastructure/helm/observability/`.

## What this is

Five containers on a private docker network:

| Component | Purpose | Local port |
|---|---|---|
| OpenTelemetry Collector | Receives OTLP from your service; fans out | 4317 (gRPC), 4318 (HTTP), 8888 (metrics), 13133 (health) |
| Prometheus | Metrics store, scrape + remote-write receiver | 9090 |
| Tempo | Distributed trace store | 3200 (HTTP) |
| Loki | Log store | 3100 |
| Grafana | Dashboards, query UI, data-source proxy | 3000 |

All ports bind to `127.0.0.1` only (D17). The stack is not exposed on the
LAN.

## First-time setup

```bash
cd infrastructure/observability-dev

# Provide a Grafana admin password — D18 requires this; the compose stack
# fail-closes if RECOR_GRAFANA_ADMIN_PASSWORD is unset.
cp .env.example .env
echo "RECOR_GRAFANA_ADMIN_PASSWORD=$(openssl rand -base64 24)" >> .env
```

`.env` is gitignored. Do not commit it.

## Daily use

```bash
# Bring up (≤30s on subsequent runs; ≤5 min first time for image pulls)
docker compose up -d

# Smoke (emits 100 traces, verifies end-to-end ingestion + Grafana proxy)
./smoke-test.sh

# Keep stack up for interactive use
RECOR_OBS_KEEP_RUNNING=1 ./smoke-test.sh

# Tear down (drops volumes)
docker compose down -v
```

Grafana opens at <http://localhost:3000>. Login with the user / password
from `.env`. The home dashboard is **RÉCOR dev stack health**.

## Emitting telemetry from your service

Point your service's OTel SDK at the collector. The collector multiplexes
traces → Tempo, metrics → Prometheus, logs → Loki.

```
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_SERVICE_NAME=my-service
OTEL_RESOURCE_ATTRIBUTES=deployment.environment=dev,service.namespace=recor
```

(`4317` is OTLP/gRPC; `4318` is OTLP/HTTP. Use whichever your SDK prefers.)

## Common queries

Grafana **Explore** → pick a data source:

- **Tempo:** TraceQL `{ resource.service.name = "my-service" }`
- **Loki:** LogQL `{service_name="my-service"} |= "error"`
- **Prometheus:** PromQL `sum(rate(otelcol_receiver_accepted_spans_total[1m])) by (service)`

## Troubleshooting

When traces don't appear:

1. `docker compose logs otel-collector | tail -50` — is the collector accepting?
2. `curl -fsS http://localhost:3200/api/echo` — is Tempo reachable?
3. `curl -fsS http://localhost:9090/api/v1/query?query=otelcol_exporter_send_failed_spans_total` — are exports failing?

When Grafana 502s:

1. `docker compose ps` — check container health
2. `docker compose logs grafana | tail -50`
3. Frequently a stale volume; `docker compose down -v` then `up -d`

The full runbook is at `docs/runbooks/observability-dev-stack.md`.

## Disk usage

First-run image pull is ~2 GB total. Persistent volumes start small and
grow with retention; the dev config caps:

- Prometheus: 1 GB / 1 day
- Tempo: implicit by retention (24 h block retention)
- Loki: implicit by retention (24 h)

`docker compose down -v` reclaims everything.

## Not the production stack

This is the dev surface. Production observability is the Helm chart at
`infrastructure/helm/observability/` deployed by ArgoCD against the
consortium's Kubernetes cluster. The component versions match (same image
tags); the deployment topology differs (HA replicas, S3 backends,
ExternalSecrets, anti-affinity).

When you tune dashboards or add panels here, mirror the change into the
Helm chart's Grafana provisioning ConfigMap before merge so dev and prod
do not drift.
