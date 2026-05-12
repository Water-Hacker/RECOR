# `infrastructure/vault/` — RÉCOR Vault skeleton (OPS-4)

A local-dev HashiCorp Vault stack plus the policies, AppRole roles,
and bootstrap script the services need.

This directory is the *skeleton* for OPS-4. Production deployment
(HA, auto-unseal, audit-log → Loki, sealed-secret bootstrap) is
tracked separately and lives in `infrastructure/helm/vault/` (TBD).

## Layout

```text
infrastructure/vault/
├── README.md                       (this file)
├── docker-compose.yaml             dev Vault (single-node, in-memory)
├── policies/
│   ├── recor-declaration.hcl       declaration read-only
│   ├── recor-verification-engine.hcl V-engine read-only
│   ├── recor-portal.hcl            portal read-only
│   └── recor-admin.hcl             bootstrap operator
└── scripts/
    └── init-dev-vault.sh           idempotent bootstrap
```

## Quick start

```bash
# 1) Copy the dev root token into your shell env.
cp .env.example .env
# .env already pins VAULT_DEV_ROOT_TOKEN_ID for dev — DO NOT change it
# without coordinating with everyone running integration tests.

# 2) Bring up dev Vault.
docker compose -f infrastructure/vault/docker-compose.yaml --env-file .env up -d

# 3) Bootstrap policies, AppRole, audit log, example secrets.
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=recor-dev-root-token        # matches .env
bash infrastructure/vault/scripts/init-dev-vault.sh

# 4) Paste the printed VAULT_ROLE_ID / VAULT_SECRET_ID into each
#    service's .env, then start the services.
```

## What gets seeded

`init-dev-vault.sh` writes deterministic dev placeholders so a fresh
bring-up is reproducible (D19). Re-running the script never produces
a divergent secret bundle — secret VALUES are fixed, only AppRole
secret-ids regenerate on each invocation (Vault's design).

## Production deployment

This directory is not the production deployment manifest. Production:

- HA cluster (Raft storage; ≥ 3 nodes across AZs).
- Auto-unseal (KMS or transit) — never use Shamir for sovereign
  infrastructure.
- Audit log exported to Loki + a second sink (filesystem on each
  node, retained per the audit retention policy).
- AppRole secret-ids issued via the response-wrapping flow, not
  printed to logs.
- Root token deleted after initial setup.

See `docs/runbooks/vault-onboarding.md` for the operator runbook.

## Doctrines

- **D14 fail-closed** — when `VAULT_ADDR` is set, the service refuses
  to start if Vault is unreachable. Never falls back to env silently.
- **D18 no secrets in code** — only the AppRole bootstrap pair lives
  in env. Everything else is Vault-stored.
- **D19 reproducible everything** — `init-dev-vault.sh` is idempotent.
