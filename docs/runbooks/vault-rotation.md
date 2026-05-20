# Runbook — Vault AppRole role-id + secret-id rotation

**Doctrine:** D18 (no secrets in code, tickets, chat, logs).
**Audit reference:** closes the Cryptography row of the MEDIUM/LOW
summary table — "Vault AppRole role-id + secret-id rotation
undocumented."

This runbook documents the cadence and procedure for rotating the
AppRole credentials each service uses to authenticate to Vault. See
also `docs/runbooks/vault-onboarding.md` for first-time setup.

## When to rotate

| Trigger | Cadence | Severity if missed |
|---|---|---|
| Scheduled rotation of `secret_id` | every 30 days | medium — secret_ids carry a TTL anyway |
| Scheduled rotation of `role_id` | every 180 days | low — role_id is the long-lived identifier |
| Operator leaves the team or loses laptop | within 24h | high — assume credential compromise |
| Suspected leak (gitleaks hit; accidental log) | within 1h | critical |
| After any production-affecting compromise drill | next business day | high |

The 30-day cadence is enforced by the `secret_id_num_uses` and
`secret_id_ttl` settings on the AppRole role definition — Vault itself
will refuse a stale `secret_id`. Operators rotate proactively so a
service never restarts under the expiring credential.

## Pre-flight

1. Confirm Vault is healthy: `vault status` returns `Initialized: true`
   and `Sealed: false`.
2. Confirm the on-call team has acknowledged the rotation in the
   change-management channel.
3. Confirm `recor-vault-client`'s metrics (`recor_vault_lookup_total`,
   `recor_vault_lookup_failures_total`) are visible in Grafana.

## Procedure — `secret_id` rotation (30-day cadence)

Per service. Run from the operator workstation with the AppRole policy
attached to your token.

```bash
# 1. Mint a new secret_id with the same TTL + use count as the old one.
NEW_SECRET_ID=$(vault write -force -field=secret_id \
    auth/approle/role/recor-<service>/secret-id)

# 2. Stash it in the per-service secret path the service will read.
vault kv put secret/recor/<service>/approle \
    role_id="<unchanged role_id>" \
    secret_id="$NEW_SECRET_ID"

# 3. Roll the service. Each replica reads the new secret_id at startup
#    via recor-vault-client::populate_from_vault.
kubectl rollout restart deployment/recor-<service>

# 4. After rollout completes AND metrics show steady-state, expire the
#    old secret_id explicitly (rather than waiting for TTL).
vault write auth/approle/role/recor-<service>/secret-id-accessor/destroy \
    secret_id_accessor=<old-accessor>
```

`<service>` is one of `declaration`, `verification-engine`,
`person-service`, `entity-service`, `audit-reconciler`, `audit-verifier`,
`worker-fabric-bridge`.

## Procedure — `role_id` rotation (180-day cadence)

Less frequent; tighter coordination because the role_id is an identity,
not a credential. Rotating it requires deploying the service with the
new role_id baked into its environment.

```bash
# 1. Mint a new AppRole role with the same policy bindings.
vault write auth/approle/role/recor-<service>-new \
    token_policies="recor-<service>" \
    secret_id_ttl=30d \
    secret_id_num_uses=0 \
    token_ttl=1h \
    token_max_ttl=4h

# 2. Read the new role_id.
NEW_ROLE_ID=$(vault read -field=role_id auth/approle/role/recor-<service>-new/role-id)

# 3. Update the deployment manifest's RECOR_VAULT_ROLE_ID env var.
#    (See infrastructure/kubernetes/<service>/deployment.yaml.)

# 4. Mint the first secret_id under the new role; store it.
NEW_SECRET_ID=$(vault write -force -field=secret_id \
    auth/approle/role/recor-<service>-new/secret-id)
vault kv put secret/recor/<service>/approle \
    role_id="$NEW_ROLE_ID" \
    secret_id="$NEW_SECRET_ID"

# 5. Deploy the service with the new role_id env var. Verify steady
#    state.

# 6. Decommission the old role.
vault delete auth/approle/role/recor-<service>
vault write -force auth/approle/role/recor-<service>-new \
    -- mv (or re-create under the canonical name)
```

## Post-rotation verification

For every service that rotated:

1. `recor_vault_lookup_total{service="<service>"}` rate steady-state.
2. `recor_vault_lookup_failures_total{service="<service>"}` rate ≈ 0
   for at least 10 minutes after rollout.
3. The service's `/healthz` returns `200` and `/readyz` returns `200`.
4. An end-to-end smoke (`tools/ci/integration-smoke.sh`) passes.
5. Log the rotation in `docs/audit/rotation-log.md` (date, service,
   operator).

## Failure modes

| Symptom | Likely cause | Remediation |
|---|---|---|
| Service crash-loops with `vault: invalid secret_id` | secret_id TTL expired between rotation steps | Re-issue secret_id; re-apply step 2; restart |
| Service crash-loops with `vault: invalid role or wrong policy` | role_id rotated but env var not updated on all replicas | `kubectl rollout restart`; verify env on every pod |
| `recor_vault_lookup_failures_total` flat-line elevated post-rotation | new policy doesn't cover all secret paths the service reads | `vault policy read recor-<service>`; compare against `infrastructure/vault-policies/<service>.hcl` |

## Rollback

If the new credentials don't work, the old `secret_id` is still valid
for the remainder of its TTL (step 4 above destroys it explicitly —
skip that step if rollback is needed). Restart the deployment with the
old environment variables; the previous secret_id remains in the
service's memory.

For role_id rotation, rollback means redeploying the previous manifest
revision with the old role_id env var. The old AppRole role is not
deleted until step 6, so rollback is a redeploy not a re-create.

## On-call hooks

- Page on: `recor_vault_lookup_failures_total` rate > 5 / 5min for any
  service (alert rule `vault_lookup_burst` in
  `alerts/recor-prometheus-rules.yaml`).
- Ticket on: a scheduled rotation overdue by more than 7 days. The
  cron job that ticks the rotation calendar fires a ticket to the
  platform-ops queue.
