# RÉCOR — Declaration service Vault policy (OPS-4)
#
# Read-only access to the secret paths the declaration service needs at
# startup. Bound to the `recor-declaration` AppRole role.
#
# Secret paths used by the declaration service:
#
#   secret/recor/declaration/database
#     - DATABASE_URL
#
#   secret/recor/declaration/relay
#     - RELAY_HMAC_SECRET              (D→V outbox relay signer)
#     - RELAY_HMAC_SECRET_OLD          (rotation window)
#
#   secret/recor/declaration/writeback
#     - WRITEBACK_HMAC_SECRET          (V→D writeback verifier)
#     - WRITEBACK_HMAC_SECRET_OLD      (rotation window)
#
#   secret/recor/declaration/oidc
#     - OIDC_ISSUER_URL                (not strictly secret but co-located
#                                       so a rotation is one operation)
#     - OIDC_AUDIENCE
#
#   secret/recor/declaration/observability
#     - LOG_REDACTION_KEY              (OPS-2 BLAKE3 keyed-MAC key)
#
# KV-v2 quirk: paths in KV-v2 are addressed as `<mount>/data/<path>`
# for read/write of the secret bundle, `<mount>/metadata/<path>` for
# delete/list. This policy grants read on `data/` only; the service
# never lists secrets or sees metadata.

path "secret/data/recor/declaration/*" {
  capabilities = ["read"]
}

# Token self-renewal and lookup — required for AppRole login to keep
# the token alive across the service's lifetime without re-logging in.
path "auth/token/renew-self" {
  capabilities = ["update"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}
