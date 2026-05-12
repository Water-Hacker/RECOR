# SPIFFE onboarding + operational runbook

Operational reference for the R-LOOP-3 SPIFFE/mTLS deployment. See
`docs/adr/0008-spiffe-mtls.md` for the design decision; this document
covers the day-2 procedures.

## Scope

| Procedure | When to use |
|---|---|
| Register a new workload | Adding a new service that needs an SVID. |
| Rotate the trust bundle | After a SPIRE CA rotation or suspected CA compromise. |
| Debug an SVID-fetch failure | A service fails to start with the bootstrap error. |
| Disable mTLS for a service | Emergency fallback during an incident; reverts to HMAC. |
| Decommission a workload | A workload retires; its SVID should no longer be issued. |

## Architecture quick-reference

- SPIRE server: `infrastructure/spire/server.conf`. Single-node in
  dev; multi-replica in prod (deferred Helm chart).
- SPIRE agent: `infrastructure/spire/agent.conf`. Workload API
  socket at `unix:///tmp/spire-agent/public/api.sock`.
- Trust domain: `recor.cm` (national-registry domain).
- Workload attestation: docker label selectors
  (`docker:label:org.spiffe.workload:<name>`) in dev;
  `k8s:sa:<namespace>:<service-account>` in prod (deferred).
- Per-service config: `AUTH_TRANSPORT={hmac|mtls|mtls-only}`. The
  three states are documented in
  `services/declaration/src/config.rs` and the mirror
  `services/verification-engine/src/config.rs`.

## Procedure: register a new workload

1. **Create a registration entry file** under
   `infrastructure/spire/registration-entries/recor-<workload>.json`.
   Use the existing `recor-declaration.json` /
   `recor-verification-engine.json` as templates. The required
   fields are:

   ```json
   {
     "spiffe_id": "spiffe://recor.cm/<workload>",
     "parent_id": "spiffe://recor.cm/spire/agent/join_token/recor-dev-agent",
     "selectors": [
       "docker:label:org.spiffe.workload:<workload>"
     ],
     "ttl": 3600,
     "dns_names": ["<workload>", "<workload>.recor.local"]
   }
   ```

2. **Declare the matching label on the application container.**
   In the workload's compose file (or k8s Pod manifest), set
   `labels.org.spiffe.workload: <workload>` so the agent's docker
   workload-attestor plugin matches the selector.

3. **Re-run the bootstrap script.** It is idempotent — existing
   entries are skipped:

   ```bash
   bash infrastructure/spire/scripts/bootstrap.sh
   ```

   The script issues `spire-server entry create` only for new
   entries; subsequent runs report "entry … already exists; skipping".

4. **Verify the workload can fetch its SVID.** Mount the SPIRE
   agent socket volume into the container and run the bundled
   `recor-spiffe` smoke (see `packages/recor-spiffe/tests/` for the
   pattern):

   ```bash
   docker exec -it <workload-container> /opt/recor/check-spiffe
   ```

5. **Wire the workload into mTLS.** Set `AUTH_TRANSPORT=mtls` in
   its environment; the composition root will bootstrap the
   Workload API client at startup and refuse to start if the
   bundle fetch fails (D14 fail-closed).

## Procedure: rotate the trust bundle

The trust bundle rotates automatically on the CA's TTL
(`server.conf::ca_ttl`, default `24h` in dev / `7d` in prod). Manual
rotation is reserved for **CA compromise** or **planned migration to
a new CA**.

1. **Generate a new CA key** on the SPIRE server side:

   ```bash
   docker compose -f infrastructure/spire/docker-compose.yaml exec spire-server \
     /opt/spire/bin/spire-server x509 mint -spiffeID spiffe://recor.cm/spire/server
   ```

2. **Update `server.conf` to use the new key** and restart the
   server. The server will publish both the old and the new bundle
   for the `default_x509_svid_ttl` window (1h in dev) so workloads
   that have not yet re-fetched their bundle continue to verify
   incoming peers.

3. **Monitor `recor_spiffe_svid_fetch_total{result}`.** Once
   `success` counters across every workload have ticked at least
   once after the rotation, the rotation is complete.

4. **Audit-log the rotation.** Record the new bundle hash in the
   security team's bundle-rotation register; the procedure mirrors
   the HMAC-secret rotation runbook
   (`docs/runbooks/hmac-secret-rotation.md`) for the operator
   discipline.

## Procedure: debug an SVID-fetch failure

Symptom: a service refuses to start with
`SPIFFE Workload API bootstrap failed — refusing to start under AUTH_TRANSPORT=mtls (D14 fail-closed)`
in the log.

1. **Check the SPIRE agent is up.**

   ```bash
   docker compose -f infrastructure/spire/docker-compose.yaml ps spire-agent
   docker compose -f infrastructure/spire/docker-compose.yaml exec spire-agent \
     /opt/spire/bin/spire-agent healthcheck \
     -socketPath /tmp/spire-agent/public/api.sock
   ```

   If the agent is unhealthy, check it has a valid join token. The
   bootstrap script writes the token into the agent's data volume;
   if the agent is in a restart loop with an
   `unauthorized` log line, re-run the bootstrap script.

2. **Check the workload's selector matches a registration entry.**

   ```bash
   docker compose -f infrastructure/spire/docker-compose.yaml exec spire-server \
     /opt/spire/bin/spire-server entry show \
     -socketPath /tmp/spire-server/private/api.sock
   ```

   If the expected entry is missing, re-run the bootstrap script.

3. **Check the container's label matches the selector.**

   ```bash
   docker inspect <container> --format '{{json .Config.Labels}}'
   ```

   The label key is `org.spiffe.workload`; the value must match
   the selector in the registration entry exactly.

4. **Check the agent socket is mounted into the container.**

   ```bash
   docker inspect <container> --format '{{json .Mounts}}' | jq
   ```

   The `spire-agent-socket` volume must be mounted at
   `/tmp/spire-agent/public` inside the container.

5. **Check the SPIFFE_SOCKET env on the workload.**

   ```bash
   docker exec <container> printenv SPIFFE_SOCKET
   ```

   Default is `unix:///tmp/spire-agent/public/api.sock`. If the
   workload was configured with a different path, the workload-api
   call will fail at the connect step.

6. **Last-resort emergency fallback.** Set `AUTH_TRANSPORT=hmac`
   on the workload's env and restart. The workload skips the
   SPIFFE bootstrap entirely. The HMAC path is the v1 production
   path; existing per-channel HMAC secrets (see
   `docs/runbooks/hmac-secret-rotation.md`) remain valid. The
   incident-response template
   (`docs/runbooks/incident-response-template.md`) governs the
   post-incident reversion to mTLS.

## Procedure: disable mTLS for a service (emergency)

Use only when the SPIFFE infrastructure is the blast radius and the
HMAC fallback is the only way to keep service-to-service traffic
flowing.

1. **Confirm HMAC secrets are still configured.** Both services
   must have a non-empty `INBOUND_HMAC_SECRET` /
   `WRITEBACK_HMAC_SECRET` in their env. Production deployments
   keep these populated during the `mtls` cutover window precisely
   for this rollback.

2. **Set `AUTH_TRANSPORT=hmac`** on both services and restart.

3. **Audit-log the change.** The "fell back to HMAC" event MUST
   trigger an incident-response review per the doctrine D14
   discipline.

4. **Investigate SPIFFE infrastructure separately.** Do NOT leave
   the fallback in place beyond the time strictly necessary to
   resolve the underlying SPIRE incident.

## Procedure: decommission a workload

1. **Remove the registration entry file** under
   `infrastructure/spire/registration-entries/`. Add a one-line
   commit comment recording the decommission.

2. **Delete the entry on the running server**:

   ```bash
   docker compose -f infrastructure/spire/docker-compose.yaml exec spire-server \
     /opt/spire/bin/spire-server entry delete \
     -entryID <entry-id> \
     -socketPath /tmp/spire-server/private/api.sock
   ```

   `<entry-id>` comes from `spire-server entry show`.

3. **Confirm the workload no longer receives an SVID.** Restart
   the agent; the workload should now log a Workload API "not
   authorized" response when it attempts a fetch.

## Metrics + alerts

Two OBS-1 counters are exported under
`http://<service>:<port>/metrics`:

- `recor_spiffe_svid_fetch_total{result=success|failure|mismatch}` —
  alert on `failure` rate > 0 for > 5 min (workload cannot bootstrap)
  or on **any** `mismatch` increment (SPIRE issued an SVID to the
  wrong workload — a configuration error that should be paged on).
- `recor_spiffe_peer_verify_total{result=success|missing|malformed|denied}` —
  alert on `denied` rate > 1/min (an unauthorised workload attempted
  to connect) or on `malformed` rate > 0 (a peer presented a
  malformed certificate — possible attack signal).

Alert wiring lives in `infrastructure/observability-dev/alert-rules.yaml`
(deferred follow-up; the metrics ship today, the rules ship with the
next observability ticket).

## See also

- `docs/adr/0008-spiffe-mtls.md` — the design decision.
- `infrastructure/spire/README.md` — overview of the dev compose.
- `docs/runbooks/hmac-secret-rotation.md` — the HMAC primitive
  this work retires.
- `packages/recor-spiffe/src/lib.rs` — the shared crate's surface.
