# Pass C, Section 9 — Cryptographic operations audit

**Audit pass:** C
**Section:** 9 (Cryptography)
**Reviewer:** Claude Opus 4.7 (1M context), Pass C
**Date:** 2026-05-13
**Standard:** every cryptographic operation MUST be (a) named, (b)
audited library + version, (c) demonstrably real (not stubbed), (d)
freshness-bound where replay is in scope, (e) context-bound where
substitution is in scope, (f) dev-substitutes clearly fenced. Deviations
are recorded as CRITICAL / HIGH / MEDIUM / LOW findings below.

---

## 9.0 Inventory + verdict summary

| # | Operation | Library + version | Real? | Freshness | Context bind | Verdict |
|---|---|---|---|---|---|---|
| 1 | Declarant Ed25519 attestation (browser) | Web Crypto API (native) | Yes | nonce_hex generated; **NOT verified server-side** | declaration_id **MISSING from canonical** | **CRITICAL** — see 9.1 |
| 2 | Ed25519 verification (server) | `ed25519_dalek` 2.2 | Yes | n/a | bytes-only | **CRITICAL** when combined with #1 |
| 3 | BLAKE3 receipt hash (declaration service) | `blake3` 1.x | Yes | n/a | declaration_id + signature_hex + nonce_hex | Pass |
| 4 | BLAKE3 receipt re-derivation (audit verifier) | `blake3` 1.x | Yes | n/a | **DIFFERENT canonical form from #3** | **CRITICAL** — see 9.3 |
| 5 | HMAC-SHA256 D→V channel | `hmac` 0.12 + `sha2` 0.10 | Yes; constant-time verify | none — **Gap G2 D↔V replay window** | body-only | High (G2 documented) |
| 6 | HMAC-SHA256 V→D writeback | same | Yes; constant-time verify | none (G2) | body-only | High (G2 documented) |
| 7 | HMAC-SHA256 dual-secret rotation | same | Yes | n/a | n/a | Pass |
| 8 | OIDC JWT verification | `recor-auth-oidc` (`jsonwebtoken` 9.x) | Yes; HMAC algs refused | `exp`, `nbf`, leeway | issuer + audience | Pass with caveat — see 9.5 |
| 9 | JWKS fetch + TTL cache | `reqwest` 0.12 | Yes | TTL refresh | url-pinned | **HIGH** — HTTPS not enforced; see 9.5 |
| 10 | SPIFFE X.509-SVID bootstrap | `recor-spiffe` skeleton | **STUB** — HTTP shim, no gRPC, rustls NOT WIRED to listener | n/a | URI-SAN parsing real | **CRITICAL** — see 9.6 |
| 11 | Fabric chaincode `PutAuditEntry` | Hyperledger Fabric contract-api-go | Yes; idempotent by event_id | n/a | event_id + decl_id + receipt_hash | Pass |
| 12 | Fabric bridge → gateway shim | reqwest 0.12 over HTTP | Yes (via shim) | n/a | bearer token only | Medium — see 9.7 |
| 13 | Vault AppRole bootstrap | hand-rolled `reqwest` client | Yes | client_token TTL inherited | bound to role | Medium — TLS not pinned; see 9.8 |
| 14 | Inference Gateway → Anthropic | reqwest 0.12 + `x-api-key` | Yes; fixture mode when key unset | n/a | base URL configurable | Low — see 9.9 |
| 15 | PII-redaction keyed MAC | `blake3` keyed_hash | Yes | per-process key | per-field policy | Pass |
| 16 | Gitleaks scan over full history | gitleaks 8.x | n/a | n/a | n/a | **PASS — no leaks across 78 commits** |

**Total CRITICAL:** 3 (9.1, 9.3, 9.6) — gating for launch.
**Total HIGH:** 2 (9.5 HTTPS scheme, G2 carried forward).
**Total MEDIUM:** 3 (9.7 Fabric shim auth, 9.8 Vault TLS, 9.9 Anthropic
echo).

---

## 9.1 CRITICAL — Ed25519 attestation: `declaration_id` not in canonical bytes

**Files:**
- `applications/declarant-portal/src/lib/crypto.ts` lines 136-149
  (`canonicalPayloadBytes`)
- `services/declaration/src/api/rest.rs` lines 531-562
  (`canonical_payload_bytes`)
- `services/declaration/src/api/grpc.rs` lines 722-757
  (`canonical_submit_bytes`)
- `services/declaration/src/api/dto.rs` lines 42-60
  (`SubmitDeclarationRequest::into_command`)

**Primitive:** Ed25519 over canonical-JSON bytes.

**Real-not-stubbed:** Yes — `ed25519_dalek::VerifyingKey::verify_strict`
is invoked at `services/declaration/src/domain/attestation.rs` line 76;
no `return Ok(())` shortcut, no `Math.random`, no setTimeout. Verified.

**Defect (CRITICAL):** the canonical bytes the declarant SIGNS over do
NOT include `declaration_id`. Server-side `canonical_payload_bytes`
matches the omission. But:

1. The browser passes `declaration_id` in the request body via
   `SubmitDeclarationRequest.declaration_id` (`api/dto.rs` line 25).
2. The aggregate-id used by the use case is taken from that field —
   `req.declaration_id.unwrap_or_default()` (`api/dto.rs` line 48).
3. The aggregate's submit-once invariant fires on `self.version > 0`
   keyed BY the declaration_id loaded from Postgres
   (`domain/aggregate.rs` line 141-143).

**Attack:** an MITM (broken TLS, JS-XSS-leak, log scrape) capturing a
signed submission can change `declaration_id` to any unused UUID,
re-broadcast the same body with the same `attestation.signature_hex`,
and the server will: (a) re-verify the signature against the canonical
bytes (the bytes don't include declaration_id, so the signature still
verifies); (b) load the aggregate for the NEW declaration_id (version=0),
allow the submit; (c) create a duplicate row, duplicate event, duplicate
receipt. The original declarant is now the cryptographic owner of a
declaration they never authorised.

**Compounding:** the `nonce_hex` IS in the canonical bytes, but no
nonce-replay store exists (`grep -RIn "nonce_seen\|seen_nonce\|nonce_repo"`
across services returns ZERO matches). The threat-model `Gap G2` is
documented for the D↔V channel but is silently the case for the
**declarant→D channel too**.

**Idempotency-key header** is the only protection (`api/rest.rs` lines
400-431) and it is OPTIONAL — clients who omit the header get the
replay-attack window.

**Doctrine breach:** D15 (cryptographic provenance) — the attestation
binds to a declared body MINUS its own identifier; the "what was signed"
is not the "what was committed" because the committed-row identifier is
substitutable.

**Fix path:**
1. Include `declaration_id` in `canonicalPayloadBytes` (browser) and
   `canonical_payload_bytes` (Rust). The browser already mints the id
   up-front in the wizard (`wizard/index.tsx` line 80) so this is a
   one-line addition on both sides.
2. Persist `nonce_hex` per principal with TTL ≥ token lifetime; reject
   replay. Or: bind the signature to `submitted_at` (server clock) so
   replay outside a small window fails the freshness check.
3. Require `Idempotency-Key` header at the API gateway (refuse the
   request without it) — defence in depth, not a substitute.

**Recommendation:** ship #1 BEFORE launch. #2 may follow as a Phase 2
deliverable bundled with G2 closure (R-LOOP-2 Kafka iat enforcement
already on the roadmap).

---

## 9.2 Ed25519 verification: `verify_strict` + algorithm gate

**Files:** `services/declaration/src/domain/attestation.rs` lines 56-80.

**Primitive:** `ed25519_dalek::VerifyingKey::verify_strict`. Strict
verification (not `verify`) enforces the canonical R-S form,
preventing malleable-signature attacks; the test
`substituted_public_key_rejects` at line 159-171 demonstrates the gate
catches a key swap.

**Algorithm gate:** the verifier checks `signature_algorithm ==
SignatureAlgorithm::Ed25519` BEFORE attempting to decode bytes (line
57-61). This forecloses any future enum variant being silently
accepted.

**Library version (workspace):** `ed25519-dalek = 2.x` (workspace
Cargo.toml pinning). The crate has been third-party audited; no known
CVEs at time of writing.

**Tests verified to exist and pass on PR builds:**
- `valid_attestation_verifies` — happy path
- `tampered_payload_rejects`
- `malformed_signature_rejects`
- `malformed_public_key_rejects`
- `substituted_public_key_rejects`

**Verdict:** the verify-side implementation is sound; the defect is on
the canonical-form side (9.1), not the verify primitive.

---

## 9.3 CRITICAL — Receipt-hash canonical-form divergence

**Files:**
- `services/declaration/src/domain/aggregate.rs` lines 471-504
  (`compute_receipt_hash`)
- `apps/audit-verifier/src/hashing.rs` (`derive_receipt_hash` +
  `canonicalise`)
- `apps/audit-verifier/src/report.rs` lines 191-201
  (`derive_hash_from_event`)

**Primitive:** BLAKE3-256 over canonical JSON, hex-encoded (64 chars).

**The two canonical forms are NOT byte-equivalent.**

**Declaration service (the writer):**
```rust
#[derive(Serialize)]
struct Canonical<'a> {
    declaration_id, entity_id, declarant_principal,
    declarant_role, kind, effective_from,
    beneficial_owners, signature_hex, nonce_hex,
}
let bytes = serde_json::to_vec(&canonical);  // declaration-order keys
```

**Audit verifier (the reader):**
```rust
fn derive_hash_from_event(payload: &JsonValue) -> String {
    let mut clone = payload.clone();
    if let Some(obj) = clone.as_object_mut() {
        obj.remove("receipt_hash_hex");
    }
    derive_receipt_hash(&clone)   // sorts keys
}
```

The verifier feeds the **stored `event_payload` JSONB column** — which
contains `submitted_at`, `correlation_id`, the full `attestation` object
(not just `signature_hex`), etc. — through a key-sorting canonicaliser.
The writer feeds a different field-subset through serde declaration
order. The two will produce different hex digests for every real
declaration.

**Smoking gun in the verifier's own tests:** `report.rs` lines 209-225
build the `projection_row` by COMPUTING the derived hash from the test
payload, then STORING the SAME derived hash in
`projection_row.receipt_hash_hex`. The Matched/Mismatch test passes
because the test rigs `receipt_hash_hex = derive_hash_from_event(payload)`
artificially. In production, the real receipt hash comes from
`compute_receipt_hash(cmd)` over the writer's smaller field set — these
will not match.

**Doctrine breach:** D15 (cryptographic provenance). The audit-witness
chaincode + audit-verifier are the load-bearing "tamper-detect" path for
G1 closure (R-DECL-9 marked CLOSED in threat-model line 180). They do
NOT work as advertised. Every legitimately-anchored event will register
as `Tampered` in the verifier.

**Fix path:** the audit-verifier MUST re-derive the hash using the
SAME canonical form as the declaration service. Either:
1. Reconstruct the `Canonical` struct from `event_payload` fields and
   serialise in declaration order.
2. Or — preferred — extract `compute_receipt_hash` into a shared
   crate (`packages/recor-receipt-hash`) so writer + verifier share
   ONE canonicaliser. The verifier doc claims it deliberately avoids
   the dependency to stay decoupled; this is the wrong tradeoff for a
   security primitive.

**Recommendation:** CRITICAL gating before launch. The threat model
should be re-opened to revert G1 from CLOSED to ACTIVE until #1 or #2
ships and the verifier's test suite asserts byte-parity against a
declaration-service-produced fixture.

---

## 9.4 HMAC-SHA256 channels (D→V, V→D)

**Files:**
- `services/declaration/src/api/internal.rs` (D→V verify, dual-secret)
- `services/verification-engine/src/api/internal.rs` lines 246-273

**Primitive:** HMAC-SHA256 over the raw request body, hex-encoded in
the `X-Recor-Signature` header. Implementation:

```rust
fn verify_hmac(secret: &str, payload: &[u8], signature_hex: &str) -> bool {
    let Ok(provided) = hex::decode(signature_hex) else { return false; };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else { return false; };
    mac.update(payload);
    mac.verify_slice(&provided).is_ok()
}
```

`mac.verify_slice` from the `hmac` 0.12 crate is **constant-time** —
the comparison uses `subtle::ConstantTimeEq` under the hood. ✓

**Dual-secret rotation** (`verify_hmac_with_rotation` lines 246-262):
the current secret is checked first; on miss the old secret is checked.
A non-empty `old_secret` accepts requests signed with either key,
allowing zero-downtime rotation. When verification succeeds against
the old key, a `tracing::warn!("inbound request verified against OLD
HMAC secret — rotation in progress")` event is emitted (line 255-257);
operators can grep this signal to know when the old key can be
retired. ✓

**Rotation runbook:** `docs/runbooks/hmac-secret-rotation.md` is
referenced from threat-model line 130. The dual-secret window is the
load-bearing primitive; the runbook closes the operator-side cleanup.

**Doctrine state:** D15 satisfied. D14 fail-closed: missing-secret →
503 Service Unavailable (declaration `internal.rs` analogue + V-engine
line 115-122). Constant-time verify: yes.

**Defect (HIGH, threat-model Gap G2):** there is no freshness check on
the envelope. The receiver does enforce idempotency-by-event_id at the
**aggregate** layer (event_id is a UUIDv7 with embedded ms timestamp,
so unique forever), but if an attacker captures a valid signed
envelope and replays it within the secret's validity window, the
verification re-evaluates as success and the aggregate's domain layer
re-applies its idempotency check. The economic effect is benign (no
duplicate event written) but the SIGNAL TO OBSERVABILITY shows a fresh
attempt that wasn't really fresh.

**Status:** documented as Gap G2 in threat-model; closure pathway is
R-LOOP-2 Kafka migration with `iat` enforcement. No new finding from
this audit; carry forward.

---

## 9.5 OIDC JWT verification

**Files:** `packages/recor-auth-oidc/src/lib.rs` (`verify`,
`verify_uncached`, `supported_alg`).

**Primitive:** `jsonwebtoken` 9.x verifies a Bearer token against a
JWKS-resolved public key. Algorithm gate (`supported_alg`, line
430-448):

```rust
match alg {
    Algorithm::RS256 | RS384 | RS512 | ES256 | ES384
    | PS256 | PS384 | PS512 | EdDSA => Ok(alg),
    Algorithm::HS256 | HS384 | HS512 => Err(VerificationError::UnsupportedAlgorithm(alg)),
}
```

HMAC algorithms are refused **before** signature decode — the canonical
JWT alg-confusion attack (where an attacker re-signs an RS256 token as
HS256 with the RS256 public-key bytes as the HMAC secret) cannot reach
the signature-verify call site. ✓ (R-DECL-1 closed; verified.)

**Standard claims:** `validate_exp = true`, `validate_nbf = true`, leeway
configurable (line 322-326). Issuer pinned to the configured value
(line 323). Audience pinned (line 324). ✓

**JWKS TTL cache:** `lookup_cached` checks `is_fresh(cache, cache_ttl)`
before returning the cached key (line 371-388). On miss, `refresh_jwks`
fetches the JWKS and atomically swaps `cache.jwks` + `cache.fetched_at`
under a `tokio::sync::RwLock` (line 390-420). ✓

**LRU token cache** (line 285-310): verified tokens are inserted with
their own `exp` as the cache TTL — so a stale cache hit is impossible
without leeway-bounded clock drift. Expired entries are evicted eagerly.
Concurrent-safety via `tokio::sync::Mutex` on the LRU. ✓

**Defect (HIGH):** the threat-model claims at line 144:

> JWKS endpoint MITM — JWKS fetch is HTTPS-only and cached with TTL;
> HTTP scheme refused at startup.

Search across the codebase for the scheme-refusal:

```
$ grep -RIn "scheme\|starts_with.*http\|https://"
    packages/recor-auth-oidc/src/lib.rs
    services/declaration/src/main.rs
    services/declaration/src/config.rs
(no enforcement found)
```

**The HTTPS scheme is NOT enforced at startup.** A `JWKS_URL=http://...`
would be accepted and the JWKS would be fetched over plaintext. The
threat-model statement is **overstated**.

**Fix path:** add to `Config::from_env`:
```rust
if !cfg.jwks_url.starts_with("https://") && cfg.environment != "dev" {
    return Err(ConfigError::JwksUrlInsecure);
}
```
Or pin TLS at the `reqwest::Client::builder().https_only(true)` level.

**Recommendation:** HIGH; close before launch. The doc + code MUST agree.

---

## 9.6 CRITICAL — SPIFFE mTLS: SVID fetched, rustls NOT wired

**Files:**
- `packages/recor-spiffe/src/workload_api.rs` lines 56-111
  (`HttpWorkloadApi`)
- `services/declaration/src/main.rs` lines 162-207
- `services/verification-engine/src/main.rs` (analogue)

**Primitive (intended):** X.509-SVID mTLS for service-to-service
authentication. The declarant Ed25519 attestation runs at the
application layer; the SPIFFE SVID is the **transport-layer** primitive
that prevents cross-service spoofing under `AUTH_TRANSPORT=mtls`.

**Actual state — STUB:**
1. The `HttpWorkloadApi` is the only `WorkloadApi` implementation. Its
   own doc-comment says (line 76-84): "For the skeleton we use a
   hand-built TCP+HTTP request to keep this crate's compile cost
   small. Production swaps the whole `HttpWorkloadApi` for the gRPC
   `WorkloadApi` impl." No gRPC impl exists.
2. The `HttpWorkloadApi` speaks **plaintext HTTP**. The SVID is
   fetched over `http://` to a wiremock-shaped endpoint.
   `parse_http_url` rejects `https://` (line 144-158).
3. The declaration service's main.rs line 174-203 fetches the SVID via
   bootstrap, increments the SVID-fetch counter, and then
   **DISCARDS THE CLIENT** with `let _spiffe = spiffe_client`. The
   TODO comment at line 191-195 acknowledges:
   > TODO(R-LOOP-3-followup): build the rustls ServerConfig +
   > ClientConfig from spiffe_client and use them to swap
   > `axum::serve` for `axum-server::tls_rustls::bind`.

The `axum::serve` listener remains plaintext HTTP. The mtls_middleware
in `packages/recor-spiffe/src/middleware.rs` is never installed on the
running service.

**Doctrine breach:**
- D17 (zero trust) — the threat-model claims SPIFFE is the
  transport-layer authenticator. It is not.
- D14 (fail-closed) — `AUTH_TRANSPORT=mtls` mode requires
  `bootstrap()` to succeed and refuses to start otherwise (line 187);
  but ALL THIS DOES is verify that the Workload API stub is reachable.
  The actual mTLS gating is bypassed because the listener is not TLS.
- The threat-model row at line 136 — "mTLS via SPIFFE narrows the trust
  boundary further than HMAC alone" — is **false in the current
  build**.

**Compounding:** the `mtls_middleware` extracts peer SPIFFE IDs from
peer certs via `peer_spiffe_id_from_cert` (`rustls_glue.rs` line 142-162),
and `enforce_peer_id` gates the allowlist. Neither code path runs
because rustls is not the transport.

**Fix path:**
1. Wire `axum-server::tls_rustls::bind_rustls` using
   `spiffe_client.server_config(&cfg.spiffe_id_self).await?`.
2. Install `mtls_middleware` on the internal-endpoint axum sub-router.
3. Replace `HttpWorkloadApi` with a gRPC client against the SPIRE
   Workload API socket. The trait abstraction is already in place
   (`WorkloadApi` trait at `workload_api.rs` line 50-54).
4. Promote `AUTH_TRANSPORT=mtls-only` to refuse plain HMAC traffic in
   production.

**Recommendation:** CRITICAL gating before launch under
`AUTH_TRANSPORT=mtls` or `mtls-only`. Today, the only real
authenticator on the inter-service path is HMAC-SHA256 (which is sound
— see 9.4 — but isn't what the architecture diagram claims).

**Mitigation acceptable for v1 launch IF AUTH_TRANSPORT=hmac is the
documented launch posture:** the mode flag exists; surface a
loud-banner doc note that mTLS is not yet active and the threat-model
must be updated to mark the SPIFFE row as Phase 2.

---

## 9.7 Fabric chaincode + bridge

**Files:**
- `chaincode/audit-witness/lib/audit_witness.go`
- `packages/fabric-bridge/src/lib.rs`
- `packages/fabric-bridge/src/transport.rs`

**Chaincode:**
- `PutAuditEntry` enforces idempotency by `event_id` (line 119-121).
  A second call returns "already exists" which the bridge translates
  to `Ok(AlreadyCommitted)`. ✓
- Validates `receipt_hash_hex` is exactly 64 lowercase hex chars
  (line 260-269). Rejects malformed inputs. ✓
- Secondary index via `CreateCompositeKey` (line 138-148). Lookup via
  `GetStateByPartialCompositeKey`. ✓
- `ListAuditEntriesForDeclaration` sorts defensively (line 224) to
  guarantee ascending event_id order across LevelDB / CouchDB
  backends. ✓
- The chaincode stores `signing_peer_attestation` byte slice but does
  NOT verify it — the comment at lines 64-67 acknowledges this is a
  store-only field, with peer identity checked by the channel policy
  at endorsement time. Acceptable; documented.

**Bridge:**
- HTTP transport to a Go-shim sidecar (`HttpTransport`, lines 82-109).
  Optional `Bearer` token in the Authorization header (line 128-131).
- The shim's URL is `gateway_url/v1/transactions/{channel}/{chaincode}`.
- Retry classification: 4xx → NonRetryable; 5xx + connect/timeout →
  Retryable; 200 with `error` field → NonRetryable; 409 → AlreadyCommitted.
  ✓

**Defect (MEDIUM, 9.7):** the bridge's bearer-token authentication is
**optional**. If `bearer_token: None` (line 86), the bridge POSTs to
the shim with no Authorization header. A misconfigured deployment that
forgets the token would silently downgrade to anonymous calls to the
chaincode-orchestration shim. Recommend hardening:

```rust
pub fn new(config: &BridgeConfig) -> Result<Self, BridgeError> {
    if config.bearer_token.as_deref().unwrap_or("").is_empty()
       && config.environment != Environment::Dev {
        return Err(BridgeError::Config("bearer token required in non-dev".into()));
    }
    ...
}
```

Plus the shim itself must HTTPS-terminate; we don't audit the shim
because it's outside this repo, but the deployment doc should require
TLS on the shim and a non-empty token.

---

## 9.8 Vault AppRole client

**File:** `packages/recor-vault-client/src/lib.rs`.

- AppRole login real (line 197-225): POST `/v1/auth/approle/login`,
  decode `client_token`, return as `SecretString`. ✓
- `read_kv2` uses `X-Vault-Token` header (line 158). ✓
- Errors all return `VaultError::*`; D14 fail-closed contract enforced
  by the service composition root.
- Secrets wrapped in `SecretString` (the `secrecy` crate) — no
  accidental `Debug` print of the token. ✓

**Defect (MEDIUM, 9.8):** `reqwest::Client::builder()` (line 129-131)
is constructed with `timeout` only. No `https_only()`. No
`tls_built_in_root_certs(true)`. No certificate-pinning. A
`VAULT_ADDR=http://localhost:8200` would be accepted and the AppRole
secret_id would be POSTed in plaintext.

**Fix path:** require `VAULT_ADDR` to start with `https://` outside dev:
```rust
if !addr.starts_with("https://") && !is_dev() {
    return Err(VaultError::AddrInsecure);
}
```
Plus `.https_only(true)` on the client builder.

**Operator-side counterpart:** the runbook at
`docs/runbooks/vault-onboarding.md` (not re-audited here) should
document the TLS requirement.

---

## 9.9 Inference Gateway (Anthropic)

**File:** `packages/recor-inference-gateway/src/lib.rs`.

- API key from `ANTHROPIC_API_KEY`, wrapped in `SecretString`. Default
  base URL `https://api.anthropic.com/v1/messages` (line 47, 70).
- `x-api-key` header (line 164) uses `expose_secret()` — necessary at
  the HTTP boundary; the surrounding redaction layer (OPS-2) does NOT
  redact request headers from the outbound side. Acceptable.
- Fixture mode when API key is empty (line 138-143) — clearly logged
  via `info!("ANTHROPIC_API_KEY unset; returning fixture response (D14
  fail-closed)")`. Returns a `FixtureResponse::vacuous(...)` which is
  obviously non-real to any downstream stage.

**Defect (LOW, 9.9):** error path at line 176 logs `body = %text` of
the upstream Anthropic response on non-2xx. Anthropic's error responses
may echo portions of the prompt content in certain error shapes (e.g.,
content-policy violations include excerpts). In RÉCOR's use case the
prompt MAY contain redacted PII or projection summaries; this log path
could surface that content to ops dashboards. Tighten by either
truncating the body to N bytes OR funneling it through the redaction
layer before tracing.

**Defect (LOW, 9.9 ii):** `ANTHROPIC_API_URL` accepts any string. Add
a scheme-check (`https://` only in non-dev) for consistency with the
other external-call sites.

---

## 9.10 PII redaction (keyed MAC)

**File:** `packages/recor-logging/src/lib.rs`.

- BLAKE3 keyed-hash with a 32-byte key, output truncated to 16 hex
  chars (`mac_short`, line 193-198). 16 hex chars = 64 bits ~= 18.4
  billion-billion preimages — sufficient correlation entropy without
  full reversibility.
- Per-field policy (`redact_field`, line 204-216): SPIFFE paths get
  their `<path>` MACed, receipt-hash gets a head…tail abbreviation,
  UUID PII fields (`UUID_PII_FIELDS` set) are MACed.
- Key sourced from `LOG_REDACTION_KEY` env var; **required outside dev**
  (`RedactionConfig::from_env` line 140-152); in dev the fallback is a
  process-local `random_key()` derived from `nanos ^ pid`. ✓ D14 enforced.

**Defect (LOW, 9.10):** the `random_key()` fallback (line 177-191)
derives "randomness" by XORing time-nanos bytes with pid bytes and
BLAKE3-hashing the result. This is NOT cryptographically random; a
local attacker who knows the process start time + PID can reconstruct
the key and reverse the MAC. Acceptable in dev (where redaction is
defence-in-depth anyway), but recommend replacing with `getrandom`
crate for forward-compatibility — if the dev-vs-prod boundary ever
slips, the key is then still strong.

---

## 9.11 Gitleaks + secret-scan audit

**Tool:** `gitleaks` 8.x via `gitleaks detect --source . --redact -v`.

**Scope:** entire git history — 78 commits from PR #1 to the
docs/audit-pass-c branch tip.

**Result:**
```
11:57AM INF 78 commits scanned.
11:57AM INF scanned ~4309188 bytes (4.31 MB) in 509ms
11:57AM INF no leaks found
```

**Evidence:** `docs/audit/evidence/cryptography/gitleaks.log`.

**Trufflehog:** not installed on the audit host; skipped. gitleaks's
default ruleset is sufficient for the standard secret-shapes
(API keys, private keys, AWS, GCP, etc.). Trufflehog adds entropy-based
scanning which can be valuable; document as a follow-up for the
production CI gate (D20 supply-chain integrity).

**Verdict:** PASS. No committed secrets across the full repository
history. The `.env.example` files contain placeholder values
(`x`, `changeme`) and are clearly labelled.

---

## 9.12 Cross-cutting: dev-vs-prod substitution discipline

The repository has multiple substitute-then-fence patterns. Audited:

| Substitute | Real production wire-up | Fence (refuse-prod) | Visible indicator | Verdict |
|---|---|---|---|---|
| `HttpWorkloadApi` (SPIFFE) | gRPC over UDS to SPIRE | None — only `AUTH_TRANSPORT=hmac` avoids using it | Log message at startup | **FAIL** — see 9.6 |
| Dev `X-Recor-Dev-Principal` header | OIDC Bearer | `cfg.is_dev() == false → header ignored` (auth.rs:78) | Log line on every use | Pass |
| Inference fixture mode | Anthropic API | `is_fixture_mode()` checks empty key; D14 OK | `info!` line, fixture response is obviously non-real | Pass |
| Mock BUNEC fixture | Real BUNEC API | R-VER-1 wires real; mock is feature-gated | (out of scope this pass) | Pass (cross-ref) |
| Random fallback log-redaction key | Vault-sourced 32-byte key | `is_dev == false → KeyRequired` error | Warn log + KeyRequired error | Pass |

**The SPIFFE substitute (9.6) is the only one whose substitute-without-indicator
posture is unsafe.** Two of the three guards listed in the task spec
(clearly labelled / multiple guards / visible indicator) are present —
the code says it's a skeleton — but the THIRD guard (refuse-to-run-prod)
is absent.

---

## 9.13 Findings ledger

| ID | Severity | Surface | Doctrine | Status |
|---|---|---|---|---|
| AUDIT-C-CRYPTO-01 | **CRITICAL** | `crypto.ts::canonicalPayloadBytes` + `rest.rs::canonical_payload_bytes` — declaration_id missing from signed bytes | D15 | **Gating** |
| AUDIT-C-CRYPTO-02 | **CRITICAL** | `report.rs::derive_hash_from_event` vs `aggregate.rs::compute_receipt_hash` — canonical-form divergence | D15 | **Gating; revert G1 to ACTIVE** |
| AUDIT-C-CRYPTO-03 | **CRITICAL** | SPIFFE mTLS bootstrap-only; rustls listener not wired | D14, D17 | **Gating under mtls/mtls-only; document if launching on `hmac`** |
| AUDIT-C-CRYPTO-04 | High | OIDC: HTTPS scheme not enforced at startup (threat-model overstated) | D14, D17 | Gating |
| AUDIT-C-CRYPTO-05 | High | Gap G2 — D↔V replay window (declarant→D too) | — | Carry forward; R-LOOP-2 |
| AUDIT-C-CRYPTO-06 | Medium | Fabric bridge bearer token optional | D14 | Backlog |
| AUDIT-C-CRYPTO-07 | Medium | Vault TLS not pinned | D17 | Backlog |
| AUDIT-C-CRYPTO-08 | Low | Inference Gateway logs upstream non-2xx body in clear | D18 | Backlog |
| AUDIT-C-CRYPTO-09 | Low | `ANTHROPIC_API_URL` accepts http:// | D17 | Backlog |
| AUDIT-C-CRYPTO-10 | Low | Dev log-redaction `random_key()` is nanos ⊕ pid | D14 | Backlog |

---

## 9.14 Evidence index

| Path | Contents |
|---|---|
| `docs/audit/evidence/cryptography/gitleaks.log` | gitleaks: 78 commits, 4.31 MB, 0 leaks |

Reproducibility:
```bash
gitleaks detect --source . --redact -v
```
