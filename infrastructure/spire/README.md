# SPIRE — RÉCOR dev deployment

This directory contains the single-node SPIRE deployment that backs
**R-LOOP-3** (SPIFFE-based mTLS for service-to-service authentication
between the Declaration service and the Verification engine).

It is **dev / integration-smoke only**. Production deployment lives
under `infrastructure/helm/spire/` (deferred to a follow-up ticket) and
uses `k8s_psat` workload attestation, KMS-backed key management, and a
multi-replica SPIRE server.

## What's here

| File | Purpose |
|---|---|
| `docker-compose.yaml` | Brings up a SPIRE server + agent (single host, docker label attestation, file-backed CA). |
| `server.conf` | Trust domain `recor.cm`; SQLite datastore; disk key manager. |
| `agent.conf` | Joins the server via a single-use token; exposes the Workload API socket at `unix:///tmp/spire-agent/public/api.sock`. |
| `registration-entries/*.json` | One file per workload — declares `(spiffe_id, selector)` pairs. The bootstrap script loads each with `spire-server entry create`. |
| `scripts/bootstrap.sh` | Idempotent: compose-up, generate join token, load registration entries. |

## Trust domain

`recor.cm` — matches the national-registry domain. Workload SPIFFE IDs:

- `spiffe://recor.cm/declaration` — the Declaration service
- `spiffe://recor.cm/verification` — the Verification engine
- `spiffe://recor.cm/portal` — the declarant portal's server-side
  companion (the browser SPA itself does **not** participate in SPIFFE)

Adding a new workload: add a JSON file under
`registration-entries/`, declare the matching `org.spiffe.workload=<value>`
label on the application container in its compose file, and re-run
`bash scripts/bootstrap.sh`. The script is idempotent — existing
entries are skipped.

## Bringing it up

```bash
bash infrastructure/spire/scripts/bootstrap.sh
```

This will:

1. `docker compose up -d spire-server` and wait for the healthcheck.
2. Generate a single-use join token via `spire-server token generate`.
3. Write the token into the agent's data volume.
4. `docker compose up -d spire-agent` and wait for its healthcheck.
5. Load every `registration-entries/*.json` into the server.

After completion, application containers that mount the
`spire-agent-socket` named volume at `/tmp/spire-agent/public` can
fetch their SVIDs via the standardised SPIFFE Workload API.

## Wiring an application into mTLS

In the service's compose file:

```yaml
services:
  declaration:
    labels:
      org.spiffe.workload: "declaration"   # matches the selector in
                                            # recor-declaration.json
    environment:
      AUTH_TRANSPORT: "mtls"                # or "mtls-only"
      SPIFFE_SOCKET: "unix:///tmp/spire-agent/public/api.sock"
    volumes:
      - spire-agent-socket:/tmp/spire-agent/public:ro
```

The service's startup path (see `services/declaration/src/main.rs`)
fetches an SVID + trust bundle via the `recor-spiffe` crate and binds
the axum server with rustls configured for mutual authentication.

## Tearing it down

```bash
docker compose -f infrastructure/spire/docker-compose.yaml down -v
```

The `-v` removes named volumes (server datastore, agent SVIDs). Omit
`-v` to preserve state across restarts.

## See also

- `docs/adr/0008-spiffe-mtls.md` — the design decision.
- `docs/runbooks/spiffe-onboarding.md` — operational procedures
  (registering a new workload, rotating the trust bundle, debugging
  SVID-fetch failures).
- `packages/recor-spiffe/` — the shared Rust crate that exposes the
  Workload API client + rustls glue.
