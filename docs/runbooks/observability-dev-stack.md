# Runbook — dev observability stack

Authoritative operating procedure for `infrastructure/observability-dev/`.
For background, see the chapter overview at `infrastructure/observability-dev/README.md`.

## When this runbook fires

- A service developer reports traces, metrics, or logs not appearing in
  Grafana
- The `observability-smoke.yaml` CI workflow fails on a PR or scheduled run
- A merged change to `infrastructure/observability-dev/**` causes the
  smoke to break locally

## First-line diagnostics

```bash
cd infrastructure/observability-dev
docker compose ps
```

Expected: five containers, all `running (healthy)`. If any is unhealthy
or restarting, jump to the per-component section below.

```bash
./smoke-test.sh
```

Smoke exits with the step number that failed. Map step → component:

| Step | Failure mode |
|---|---|
| 1 | `docker compose up -d` failed; usually port conflict (3000, 9090, 3200, 3100, 4317, 4318) or missing `RECOR_GRAFANA_ADMIN_PASSWORD` |
| 2 | A container failed its healthcheck within 60 s; see per-component section |
| 3 | A health endpoint returned non-200; component is up but degraded |
| 4 | `telemetrygen` image pull failed (no internet) or could not reach `otel-collector:4317` |
| 5 | Tempo did not ingest within 30 s; usually OTel Collector → Tempo path broken |
| 6 | Grafana proxy denied the request; usually wrong admin password |

## OTel Collector won't start / restarts in a loop

```bash
docker compose logs otel-collector | tail -100
```

Common causes:

- **YAML parse error in `otel-collector-config.yaml`.** The collector logs
  `error decoding 'config'`. Fix the YAML, then `docker compose restart
  otel-collector`.
- **Receiver port already in use.** Logs show `bind: address already in
  use`. Stop the conflicting process, or override the host-side port in
  `docker-compose.yaml`.
- **Memory limit hit.** Logs show `memory limiter killing batches`. Raise
  `processors.memory_limiter.limit_percentage` if the host has headroom;
  otherwise reduce telemetry volume.

## Prometheus won't accept remote-write

```bash
docker compose logs prometheus | tail -50
```

- **`--web.enable-remote-write-receiver` missing.** Confirm the
  `docker-compose.yaml` command list includes the flag.
- **Disk full.** Prometheus is mounted on the named volume
  `prometheus-data`. Retention is configured to 1 GB; if the volume
  grew, `docker volume rm recor-observability-dev_prometheus-data` and
  restart.

## Tempo won't ingest traces

```bash
docker compose logs tempo | tail -100
```

- **WAL or blocks dir permissions.** Tempo runs as root in the dev image
  but the local volume mount may have host-uid ownership. Fix:
  `docker compose down -v` and recreate.
- **OTel exporter target wrong.** In `otel-collector-config.yaml`, the
  `otlp/tempo` exporter must point at `tempo:4317` (service name, not
  `localhost`).

## Loki won't ingest logs

```bash
docker compose logs loki | tail -100
```

- **`allow_structured_metadata` flag.** Loki 3.x requires
  `limits_config.allow_structured_metadata: true` when ingesting OTLP
  data with attributes; the dev config sets this. If you've forked,
  re-check.
- **Schema config date in the future.** The dev config's `from:
  "2026-01-01"` must be ≤ today's date; if you set a future date, Loki
  refuses writes.

## Grafana 401 / 403 on data-source proxy

The admin password from `.env` must match. To rotate:

```bash
docker compose down
echo "RECOR_GRAFANA_ADMIN_PASSWORD=$(openssl rand -base64 24)" > .env
docker compose up -d
```

## Grafana data sources empty

Data sources are provisioned from
`grafana/provisioning/datasources/datasources.yaml` on container start.
If they're missing:

- Check the volume mount: `docker compose exec grafana ls
  /etc/grafana/provisioning/datasources`
- Look for parse errors:
  `docker compose logs grafana | grep -i provisioning`

## Full reset

When the diagnostic path doesn't converge:

```bash
docker compose down -v --remove-orphans
docker compose pull
docker compose up -d
./smoke-test.sh
```

This wipes volumes and re-pulls images. After this, if smoke still fails,
the defect is in the committed configuration; escalate via PR.

## Image upgrades

Image versions are pinned in `docker-compose.yaml`. Bumps follow the
dependency-upgrade policy in Architecture V3 P7. The general procedure:

1. Update the tag in `docker-compose.yaml`
2. Mirror the bump in `infrastructure/helm/observability/values.yaml` (or
   the relevant sub-chart's image tag)
3. Re-run smoke locally
4. Open PR; the `observability-smoke` CI workflow re-runs against the
   new images

## Production analogue

This runbook covers the dev compose stack. The production cluster
deployment uses the Helm chart at `infrastructure/helm/observability/` and
ArgoCD-managed reconciliation. The production runbook is a future ticket;
when authored, it will be at `docs/runbooks/observability-prod-stack.md`.
