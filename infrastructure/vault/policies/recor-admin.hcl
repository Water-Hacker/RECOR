# RÉCOR — Bootstrap operator Vault policy (OPS-4)
#
# Full access to the recor/* namespace plus the auth + sys mounts
# required to manage AppRole roles, enable audit devices, and rotate
# encryption keys. Bound to the operator who runs init-dev-vault.sh and
# subsequent rotation steps.
#
# This policy intentionally does NOT grant `sys/seal` — sealing a dev
# Vault is the operator's job via the CLI directly with root, not via
# the admin role. In production the seal/unseal flow uses Vault's own
# Shamir / auto-unseal infrastructure and is documented separately
# (docs/runbooks/vault-onboarding.md § Production deployment
# expectations).
#
# WHY NOT just use the root token in dev?
#   - Even in dev we want to exercise the same RBAC plumbing prod uses.
#   - The root token is logged at compose startup (dev mode); narrowing
#     the operator surface to admin-policy reduces the audit-log noise
#     and makes "what action did the operator take" easier to grep.

# Full access to the secret/recor/* tree.
path "secret/data/recor/*" {
  capabilities = ["create", "read", "update", "delete", "list", "patch"]
}

path "secret/metadata/recor/*" {
  capabilities = ["create", "read", "update", "delete", "list", "patch"]
}

# Manage AppRole auth method: enable, create roles, fetch role-ids and
# secret-ids.
path "auth/approle/*" {
  capabilities = ["create", "read", "update", "delete", "list"]
}

# Manage policies.
path "sys/policies/acl/*" {
  capabilities = ["create", "read", "update", "delete", "list"]
}

# Read the list of auth methods + secret mounts (needed to detect
# whether init has already run — idempotent bootstrap).
path "sys/auth" {
  capabilities = ["read"]
}

path "sys/auth/*" {
  capabilities = ["create", "read", "update", "delete", "sudo"]
}

path "sys/mounts" {
  capabilities = ["read"]
}

path "sys/mounts/*" {
  capabilities = ["create", "read", "update", "delete", "sudo"]
}

# Audit device management.
path "sys/audit" {
  capabilities = ["read", "sudo"]
}

path "sys/audit/*" {
  capabilities = ["create", "read", "update", "delete", "sudo"]
}

# Health / capabilities introspection.
path "sys/health" {
  capabilities = ["read"]
}

path "sys/capabilities-self" {
  capabilities = ["update"]
}

# Token self-management.
path "auth/token/renew-self" {
  capabilities = ["update"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}
