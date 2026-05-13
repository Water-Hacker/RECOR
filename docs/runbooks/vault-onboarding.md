# Runbook: Vault Onboarding

**Tracks:** OPS-4. Establishes how RÉCOR services discover their
secrets at startup, how to add a new secret, how to rotate one, and
how to onboard a new service to the Vault flow.

## Scope

This runbook covers:

1. Bringing up the dev Vault stack on a developer laptop.
2. Writing a new secret into Vault.
3. Rotating a secret with the dual-secret pattern from
   [hmac-secret-rotation](hmac-secret-rotation.md).
4. Onboarding a new service to Vault.
5. Production deployment expectations (what we run in dev vs what we
   run in production — they are *not* the same thing).

## Architecture, in one paragraph

Each service has an AppRole bound to a read-only policy on a
service-scoped subtree of `secret/recor/<service>/`. At startup the
service reads `VAULT_ADDR`, `VAULT_ROLE_ID`, and `VAULT_SECRET_ID`
from env — these three (and only these three) are env-borne. The
`recor-vault-client` crate logs in via AppRole, pulls the secrets
listed in the service's `main.rs`, injects them into process env,
and the existing `Config::from_env()` runs unchanged. When
`VAULT_ADDR` is empty, the service falls back to env-only mode with
a startup `warn!`. When `VAULT_ADDR` is set and Vault is unreachable
or refuses the AppRole login, the service refuses to start (D14
fail-closed).

## 1. Bring up dev Vault

```bash
# From repo root.
cp .env.example .env
# .env now contains VAULT_DEV_ROOT_TOKEN_ID=recor-dev-root-token

docker compose -f infrastructure/vault/docker-compose.yaml \
    --env-file .env \
    up -d
```

Vault is bound to `127.0.0.1:8200` only. The dev root token is fixed
at `recor-dev-root-token` so every developer's bootstrap script
behaves identically (D19 reproducibility). DO NOT change this token
without coordinating with everyone running integration tests.

Bootstrap policies, AppRole roles, audit log, and example secrets:

```bash
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=recor-dev-root-token
bash infrastructure/vault/scripts/init-dev-vault.sh
```

The script is idempotent. Running it twice on the same Vault
instance produces no spurious diffs. On a freshly-restarted Vault
(dev mode wipes state on restart) the script produces byte-identical
secret values for the seeded paths — only the AppRole secret-ids
regenerate.

The script prints the three role-id / secret-id pairs to stdout —
paste each into the matching service's `.env`:

```text
# services/declaration/.env
VAULT_ADDR=http://127.0.0.1:8200
VAULT_ROLE_ID=<paste from init script>
VAULT_SECRET_ID=<paste from init script>
```

Restart the service; it now pulls secrets from Vault.

## 2. Write a new secret

A new secret is written under `secret/recor/<service>/<group>` where
`<group>` is one of the per-service subtrees defined in the service's
Vault policy (e.g. `database`, `relay`, `writeback`, `oidc`,
`observability`).

Example — adding a Kafka SASL password to the declaration service:

```bash
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=recor-dev-root-token   # dev; prod uses the admin AppRole

vault kv put secret/recor/declaration/kafka \
    SASL_USERNAME="recor-declaration" \
    SASL_PASSWORD="$(openssl rand -hex 32)"
```

Three follow-ups in the same PR:

1. **Policy update.** Add the path glob to
   `infrastructure/vault/policies/recor-declaration.hcl` if the new
   group is outside the existing `secret/data/recor/declaration/*`
   glob (it isn't, in this example, but adding a new top-level
   subtree would require a policy patch).
2. **Service wiring.** Extend the `vault_paths` slice in the
   service's `main.rs` to include the new path + key → env-var
   mapping.
3. **Documentation.** Add the new env var to the service's
   `Config` struct and `.env.example`.

D18: never include the secret value in the PR, in the ticket, in
chat, or in logs. The PR cites the path; the value lives in Vault.

## 3. Rotate a secret

The rotation primitive RÉCOR uses for HMAC keys is the dual-secret
pattern from [hmac-secret-rotation](hmac-secret-rotation.md): the
verifier accepts both `*_HMAC_SECRET` and `*_HMAC_SECRET_OLD` during
the rotation window. With Vault, the same primitive applies but the
operator updates Vault paths instead of env vars.

### Rotating the D→V HMAC secret (example)

1. Generate the new secret:

   ```bash
   NEW_SECRET=$(openssl rand -hex 32)
   ```

2. Update the V-engine verifier path so it accepts both secrets:

   ```bash
   # Snapshot current → OLD.
   OLD_SECRET=$(vault kv get -field=INBOUND_HMAC_SECRET secret/recor/verification-engine/inbound)
   vault kv put secret/recor/verification-engine/inbound \
       INBOUND_HMAC_SECRET="${NEW_SECRET}" \
       INBOUND_HMAC_SECRET_OLD="${OLD_SECRET}"
   ```

3. Restart the V-engine pods (Vault reads happen at startup; the
   service does not poll Vault). The verifier now accepts both.

4. Update the Declaration signer path to the new secret:

   ```bash
   vault kv put secret/recor/declaration/relay \
       RELAY_HMAC_SECRET="${NEW_SECRET}" \
       RELAY_HMAC_SECRET_OLD=""
   ```

5. Restart the Declaration pods. The signer now signs with the new
   secret; in-flight requests already signed with the old secret
   still verify on the V-engine side.

6. Wait the drain window (~90s with default settings — see
   [hmac-secret-rotation](hmac-secret-rotation.md)).

7. Clear the OLD value on the V-engine path:

   ```bash
   vault kv put secret/recor/verification-engine/inbound \
       INBOUND_HMAC_SECRET="${NEW_SECRET}" \
       INBOUND_HMAC_SECRET_OLD=""
   ```

8. Restart the V-engine pods. The verifier now accepts only the new
   secret. Rotation complete.

### Rotating other secrets

| Secret | Path | Procedure |
|---|---|---|
| `DATABASE_URL` | `secret/recor/<service>/database` | Coordinate with the DB password rotation — see `docs/runbooks/restore-database-from-backup.md` |
| `LOG_REDACTION_KEY` | `secret/recor/<service>/observability` | Per-service; do not rotate both services simultaneously (the keyed-MAC anonymity set rebuilds on rotation) |
| OIDC client secret (future) | `secret/recor/<service>/oidc` | Coordinate with the IdP team |

## 4. Onboard a new service

To bring a new service onto the Vault flow:

1. **Decide the subtree.** Pick a non-overlapping path under
   `secret/recor/<new-service>/`. Convention: one path per logical
   secret group (`database`, `messaging`, etc.).

2. **Author the policy.** Create
   `infrastructure/vault/policies/recor-<new-service>.hcl` modelled
   on `recor-declaration.hcl`. Keep capabilities to `read` only.

3. **Register in `init-dev-vault.sh`.** Add a `create_role` call and
   a `print_credentials` call for the new role. Add `vault kv put`
   stanzas seeding the dev placeholders.

4. **Wire the service.** Add `recor-vault-client` to the new
   service's `Cargo.toml` and call `populate_from_vault` from its
   `main.rs` with the path → env-var mapping.

5. **Document the env contract.** Update the service's
   `.env.example` to include `VAULT_ADDR`, `VAULT_ROLE_ID`,
   `VAULT_SECRET_ID` with the "optional in dev / required in prod"
   comment.

6. **Update this runbook.** Add the service to the table in § 5
   below.

## 5. Production deployment expectations

The compose file in `infrastructure/vault/docker-compose.yaml` is
**not** the production deployment. Production:

- **HA cluster.** Raft storage backend, ≥ 3 nodes across at least 2
  availability zones. No single-node prod ever.
- **Auto-unseal.** A cloud-KMS or transit-based auto-unseal so a
  restarted node returns to service without manual key-shard
  ceremony. Shamir is acceptable as a break-glass fallback but the
  primary unseal path is auto-unseal. The five Shamir key shares,
  if used, are split between five named operators across the
  consortium (the operator identity list lives in the secrets
  custody appendix of the Architecture document).
- **Audit log → Loki.** The file audit device's audit.log is
  shipped to Loki via promtail with a parallel filesystem retention
  on each Vault node. Vault refuses to serve requests if ALL audit
  devices fail — so we always run at least two (the file device for
  forensics, a second for redundancy).
- **AppRole secret-id distribution.** Production uses Vault's
  response-wrapping flow: a one-time-use wrapping token delivers
  the secret-id to the consumer, never printed to logs.
- **Root token destroyed.** The initial root token from cluster
  initialisation is revoked after the admin AppRole is set up. The
  break-glass path uses the Shamir shares + recovery key.
- **Network posture.** Vault binds to the internal mesh only; the
  external boundary terminates at the platform's API gateway. The
  Vault API is never directly reachable from the public internet.
  Cluster-internal traffic is mTLS.
- **Backup.** Raft snapshots taken hourly to an offsite encrypted
  store. Restore is exercised quarterly (drill noted in
  [restore-database-from-backup](restore-database-from-backup.md)'s
  cousin runbook for the Vault snapshot).
- **Monitoring.** Vault's own metrics endpoint scraped by
  Prometheus (configured under OBS-1). Alerts on: seal state ≠
  active, audit-log write failures, AppRole login failure rate
  spike, token revocation rate spike.

### Service inventory

| Service | AppRole role | Policy file | Subtree |
|---|---|---|---|
| `recor-declaration` | `recor-declaration` | `policies/recor-declaration.hcl` | `secret/recor/declaration/*` |
| `recor-verification-engine` | `recor-verification-engine` | `policies/recor-verification-engine.hcl` | `secret/recor/verification-engine/*` |
| `recor-declarant-portal` | `recor-portal` | `policies/recor-portal.hcl` | `secret/recor/portal/*` |

## On-call quick-reference

| Symptom | Likely cause | Action |
|---|---|---|
| Service refuses to start: `Vault secret loading failed` | Vault unreachable or AppRole invalid | Verify `VAULT_ADDR` reaches the cluster; confirm `VAULT_ROLE_ID` + `VAULT_SECRET_ID` haven't expired; re-issue secret-id via the bootstrap operator |
| Service starts with `VAULT_ADDR is empty — falling back to env-only` warn | Vault integration disabled | Production: fix immediately (D18). Dev: expected when running off raw env |
| Service starts but DB connection fails with empty `DATABASE_URL` | KV-v2 path missing the key | Verify with `vault kv get secret/recor/<service>/database` — the `DATABASE_URL` key must exist |
| `403 permission denied` on a Vault read | Policy path mismatch | Inspect the policy with `vault policy read recor-<service>` and confirm the policy globs cover the path |
| AppRole secret-id expired | 24h TTL elapsed | Re-issue with `vault write -force -field=secret_id auth/approle/role/<role>/secret-id` |

## Related runbooks

- [hmac-secret-rotation](hmac-secret-rotation.md) — the dual-secret
  pattern this runbook layers on top of for HMAC keys
- [oncall-triage-tree](oncall-triage-tree.md) — entry point for the
  "service won't start" symptom
- [restore-database-from-backup](restore-database-from-backup.md) —
  coordinates with `DATABASE_URL` rotation
- [observability-prod-stack](observability-prod-stack.md) — where
  Vault audit logs land in production (Loki)
- [supply-chain](supply-chain.md) — what gates exist around the
  Vault image and its provenance
