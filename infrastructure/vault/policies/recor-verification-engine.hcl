# RÉCOR — Verification Engine Vault policy (OPS-4)
#
# Read-only access to the V-engine's secret paths. Bound to the
# `recor-verification-engine` AppRole role.
#
# Secret paths used by the V-engine:
#
#   secret/recor/verification-engine/database
#     - DATABASE_URL
#
#   secret/recor/verification-engine/inbound
#     - INBOUND_HMAC_SECRET            (D→V verifier; signer = declaration's RELAY_HMAC_SECRET)
#     - INBOUND_HMAC_SECRET_OLD        (rotation window)
#
#   secret/recor/verification-engine/writeback
#     - WRITEBACK_HMAC_SECRET          (V→D signer; verifier = declaration's WRITEBACK_HMAC_SECRET)
#
#   secret/recor/verification-engine/oidc
#     - OIDC_ISSUER_URL
#     - OIDC_AUDIENCE
#
#   secret/recor/verification-engine/observability
#     - LOG_REDACTION_KEY              (independent key from declaration's;
#                                       the redaction key is per-service so
#                                       leaking one keyed-MAC inventory does
#                                       not deanonymise the other)
#
# A rotation of one service's LOG_REDACTION_KEY does NOT cascade: the
# operator rotates per service. The runbook documents both procedures.

path "secret/data/recor/verification-engine/*" {
  capabilities = ["read"]
}

path "auth/token/renew-self" {
  capabilities = ["update"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}
