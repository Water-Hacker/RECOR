# RÉCOR — Declarant Portal Vault policy (OPS-4)
#
# Read-only access to the secret paths the portal needs at container
# startup (via the portal's docker-entrypoint.sh, which can pull from
# Vault before invoking envsubst on the nginx templates).
#
# Secret paths used by the portal:
#
#   secret/recor/portal/csp
#     - CSP_CONNECT_SRC               (origins the SPA may reach)
#
#   secret/recor/portal/oidc
#     - OIDC_CLIENT_ID                (public OIDC client id; not strictly
#                                      secret in the OAuth spec sense, but
#                                      co-located in Vault for one-stop
#                                      rotation when the IdP rotates clients)
#     - OIDC_ISSUER_URL
#
# The portal does NOT need any HMAC or database secrets — it is a pure
# static asset bundle plus a thin nginx config. The policy stays tight
# so a portal-pod compromise does not give an attacker the service
# secrets.

path "secret/data/recor/portal/*" {
  capabilities = ["read"]
}

path "auth/token/renew-self" {
  capabilities = ["update"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}
