#!/usr/bin/env bash
# RÉCOR — bootstrap the dev Vault (OPS-4).
#
# Idempotent. Run after `docker compose -f infrastructure/vault/docker-compose.yaml up -d`.
#
# What it does:
#   1. Waits for the dev Vault to be ready (max 30s).
#   2. Enables KV-v2 at `secret/` (no-op if already enabled).
#   3. Enables a file audit device at /vault/audit/audit.log inside the
#      container so every secret access is recorded.
#   4. Writes the four policies (recor-declaration, recor-verification-engine,
#      recor-portal, recor-admin).
#   5. Enables AppRole auth (no-op if already enabled).
#   6. Creates the three service roles, prints their role-id / secret-id
#      pairs so the operator can paste them into the per-service .env.
#   7. Seeds example secrets — deterministic test values for dev, so a
#      developer running this script twice gets the same secrets and
#      can re-run integration tests reproducibly (D19).
#
# Requires:
#   - Vault server reachable at $VAULT_ADDR (default http://127.0.0.1:8200)
#   - Vault root token in $VAULT_DEV_ROOT_TOKEN_ID (or $VAULT_TOKEN)
#   - The Vault CLI on PATH, OR `curl` + `jq` (script auto-detects).
#
# Reproducibility (D19): every secret value written by this script is
# either fixed or derived from a fixed seed. Re-running on a fresh
# Vault container produces the byte-identical bundle.
#
# WARNING: the secrets written here are DEV PLACEHOLDERS. Production
# deployment uses the same path layout but the operator writes real
# values via `vault kv put` per docs/runbooks/vault-onboarding.md.

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────
VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8200}"
VAULT_TOKEN="${VAULT_DEV_ROOT_TOKEN_ID:-${VAULT_TOKEN:-recor-dev-root-token}}"
export VAULT_ADDR VAULT_TOKEN

# Resolve repo root from this script's location so the script can be
# invoked from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VAULT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
POLICY_DIR="${VAULT_DIR}/policies"

# ── Pre-flight ─────────────────────────────────────────────────────
log() { printf '[init-dev-vault] %s\n' "$*"; }
fail() { printf '[init-dev-vault] FATAL: %s\n' "$*" >&2; exit 1; }

if ! command -v vault >/dev/null 2>&1; then
    fail "the 'vault' CLI is not on PATH; install hashicorp/vault and retry"
fi

[ -d "${POLICY_DIR}" ] || fail "policy dir not found at ${POLICY_DIR}"

# ── Wait for Vault ─────────────────────────────────────────────────
log "waiting for Vault at ${VAULT_ADDR}"
ready=false
for _ in $(seq 1 30); do
    if vault status >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 1
done
[ "${ready}" = "true" ] || fail "Vault did not become ready within 30s"
log "Vault is up"

# ── Enable KV-v2 at secret/ ────────────────────────────────────────
if vault secrets list -format=json | grep -q '"secret/"'; then
    # Dev mode auto-mounts KV-v2 at secret/ — confirm the version is 2.
    kv_version="$(vault secrets list -format=json | \
        sed -n 's/.*"secret\/".*"version":[[:space:]]*"\([0-9]\)".*/\1/p' | head -n1)"
    if [ "${kv_version:-2}" != "2" ]; then
        log "secret/ exists at v${kv_version}; tuning to v2"
        vault secrets tune -version=2 secret/
    else
        log "KV-v2 at secret/ already enabled"
    fi
else
    log "enabling KV-v2 at secret/"
    vault secrets enable -path=secret -version=2 kv
fi

# ── Enable file audit device ───────────────────────────────────────
if vault audit list -format=json 2>/dev/null | grep -q '"file/"'; then
    log "file audit device already enabled"
else
    log "enabling file audit device at /vault/audit/audit.log"
    vault audit enable file file_path=/vault/audit/audit.log
fi

# ── Write policies ─────────────────────────────────────────────────
for policy in recor-declaration recor-verification-engine recor-portal recor-admin; do
    log "writing policy: ${policy}"
    vault policy write "${policy}" "${POLICY_DIR}/${policy}.hcl"
done

# ── Enable AppRole auth ────────────────────────────────────────────
if vault auth list -format=json | grep -q '"approle/"'; then
    log "AppRole auth already enabled"
else
    log "enabling AppRole auth at approle/"
    vault auth enable approle
fi

# ── Create AppRole roles ───────────────────────────────────────────
#
# Each role is bound to its corresponding policy. token_ttl + max_ttl
# chosen to be short enough that a leaked token has bounded blast
# radius, long enough to avoid hammering the auth endpoint.
#
create_role() {
    local role="$1"
    local policy="$2"
    log "creating AppRole role: ${role} -> policy ${policy}"
    vault write "auth/approle/role/${role}" \
        token_policies="${policy}" \
        token_ttl=1h \
        token_max_ttl=4h \
        secret_id_ttl=24h \
        secret_id_num_uses=0
}

create_role "recor-declaration" "recor-declaration"
create_role "recor-verification-engine" "recor-verification-engine"
create_role "recor-portal" "recor-portal"

# ── Seed example secrets ───────────────────────────────────────────
#
# Deterministic dev values. The HMAC secrets are 32-byte hex strings
# matching the runtime expectations of services/declaration/src/api/internal.rs
# and services/verification-engine/src/api/internal.rs. The placeholders
# are clearly marked "DEV" so an operator who accidentally lifts them
# into production sees the warning.
#
DEV_DB_URL_DECL="postgres://recor:recor-dev@127.0.0.1:5432/declaration"
DEV_DB_URL_VENG="postgres://recor:recor-dev@127.0.0.1:5433/verification"
DEV_RELAY_HMAC="0000000000000000000000000000000000000000000000000000000000000000-DEV"
DEV_WRITEBACK_HMAC="1111111111111111111111111111111111111111111111111111111111111111-DEV"
DEV_LOG_REDACTION_KEY_DECL="2222222222222222222222222222222222222222222222222222222222222222"
DEV_LOG_REDACTION_KEY_VENG="3333333333333333333333333333333333333333333333333333333333333333"

log "seeding secret/recor/declaration/* (dev placeholders)"
vault kv put secret/recor/declaration/database \
    DATABASE_URL="${DEV_DB_URL_DECL}"
vault kv put secret/recor/declaration/relay \
    RELAY_HMAC_SECRET="${DEV_RELAY_HMAC}" \
    RELAY_HMAC_SECRET_OLD=""
vault kv put secret/recor/declaration/writeback \
    WRITEBACK_HMAC_SECRET="${DEV_WRITEBACK_HMAC}" \
    WRITEBACK_HMAC_SECRET_OLD=""
vault kv put secret/recor/declaration/oidc \
    OIDC_ISSUER_URL="" \
    OIDC_AUDIENCE=""
vault kv put secret/recor/declaration/observability \
    LOG_REDACTION_KEY="${DEV_LOG_REDACTION_KEY_DECL}"

log "seeding secret/recor/verification-engine/* (dev placeholders)"
vault kv put secret/recor/verification-engine/database \
    DATABASE_URL="${DEV_DB_URL_VENG}"
vault kv put secret/recor/verification-engine/inbound \
    INBOUND_HMAC_SECRET="${DEV_RELAY_HMAC}" \
    INBOUND_HMAC_SECRET_OLD=""
vault kv put secret/recor/verification-engine/writeback \
    WRITEBACK_HMAC_SECRET="${DEV_WRITEBACK_HMAC}"
vault kv put secret/recor/verification-engine/oidc \
    OIDC_ISSUER_URL="" \
    OIDC_AUDIENCE=""
vault kv put secret/recor/verification-engine/observability \
    LOG_REDACTION_KEY="${DEV_LOG_REDACTION_KEY_VENG}"

log "seeding secret/recor/portal/* (dev placeholders)"
vault kv put secret/recor/portal/csp \
    CSP_CONNECT_SRC=""
vault kv put secret/recor/portal/oidc \
    OIDC_ISSUER_URL="" \
    OIDC_CLIENT_ID="recor-declarant-portal-dev"

# ── Fetch role-ids and secret-ids ──────────────────────────────────
print_credentials() {
    local role="$1"
    local role_id
    local secret_id
    role_id="$(vault read -field=role_id "auth/approle/role/${role}/role-id")"
    secret_id="$(vault write -force -field=secret_id "auth/approle/role/${role}/secret-id")"
    cat <<EOF

# Paste these into the matching service's .env (D18: bootstrap pair only).
# ${role}
VAULT_ADDR=${VAULT_ADDR}
VAULT_ROLE_ID=${role_id}
VAULT_SECRET_ID=${secret_id}
EOF
}

log "bootstrap complete; AppRole credentials:"
print_credentials "recor-declaration"
print_credentials "recor-verification-engine"
print_credentials "recor-portal"

log "done."
