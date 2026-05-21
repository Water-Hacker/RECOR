# FIPS-mode build (TODO-051)

## Status

Shipped, disabled-by-default. Activated by building any RÉCOR Rust
service with `--features fips`. The CI matrix
`build / fips-matrix` in `.github/workflows/required-checks.yaml`
builds the workspace under both feature sets on every PR; merge is
blocked if the `fips`-flavoured build does not compile or its unit
tests do not pass.

## Assurance level

RÉCOR's FIPS-mode build targets **FIPS-140-2 Level 1** via the
`ring` cryptographic substrate operating in its certified
algorithm set:

- AES-128-GCM, AES-256-GCM (block cipher; AEAD)
- SHA-256, SHA-384, SHA-512 (digest)
- HMAC-SHA-256, HMAC-SHA-384 (MAC)
- ECDSA over P-256 / P-384 (signature)
- ECDH over P-256 / P-384 (key agreement)
- HKDF-SHA-256, HKDF-SHA-384 (KDF)

`ring`'s upstream FIPS programme (`ring`'s `aws-lc-rs` sibling
crate at the same SemVer line) carries the active 140-2 module
certification; this is the cryptographic substrate the
`fips`-flavoured build links against. The platform's TLS layer
(`rustls 0.23`) is configured with the `ring` backend and the
cipher-suite list pinned to the FIPS-approved set:

- `TLS_AES_256_GCM_SHA384`
- `TLS_AES_128_GCM_SHA256`
- `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384`
- `TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256`
- `TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384`
- `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`

ChaCha20-Poly1305 is intentionally excluded — it is not on the
FIPS-approved AEAD list. TLS 1.3 is the floor; the `fips` build
refuses TLS 1.1 / 1.0 / SSLv3 at the rustls config layer.

## How to enable

### Local build

```bash
cargo build --workspace --release --features fips
cargo test  --workspace --lib    --features fips
```

### Container build

The production Dockerfile honours the `CARGO_FEATURES` build arg:

```bash
docker build \
  --build-arg CARGO_FEATURES=fips \
  -f services/declaration/Dockerfile \
  -t recor/declaration:fips \
  .
```

When `CARGO_FEATURES` is set, `cargo build` is invoked as
`cargo build --release --features "$CARGO_FEATURES"`.

### CI

The `build / fips-matrix` job runs unconditionally on every PR. A
red `fips`-flavoured run blocks merge — fail-closed (D14).

## Operational contract

In `fips` mode the service:

1. Refuses to use any non-FIPS cipher suite. A TLS handshake that
   would otherwise negotiate ChaCha20-Poly1305 fails closed with
   `tls_no_supported_versions_for_application_protocol`.
2. Refuses to use SHA-1 anywhere in the signature path. Legacy peer
   certificates signed with SHA-1 are refused at handshake.
3. Logs `crypto.substrate=ring-fips` on startup. The
   `governance / commit-signing` runbook references this string to
   confirm the deployed binary is the FIPS variant.

## Limitations

- The `fips` feature controls cipher-suite *selection*, not
  algorithm *availability*. A future feature-bump (TODO-051-stage-2)
  will refuse to compile if non-FIPS cryptographic crates appear in
  the dependency graph — today the bans live in `deny.toml § bans`.
- Hardware Security Module (HSM) operator-key custody is out of
  scope for this stage. The platform's signing keys today live in
  Vault transit; HSM custody is `OPS-7` (deferred).
- Post-quantum hybrid (TODO-052) is composable with FIPS: enabling
  both feature flags brings up the ML-KEM-768 hybrid handshake AND
  pins the AEAD to the FIPS-approved suite-set. Note that ML-KEM is
  not (yet) on the FIPS-140-3 approved algorithm list as of the
  document date; the hybrid path is therefore experimental in this
  configuration and operators in regulated environments should
  consult their assessor before turning both flags on simultaneously.

## Verification

```bash
# Confirm the binary is the fips variant
./target/release/recor-declaration --version | grep 'fips'

# Run the fips-only unit suite
cargo test --workspace --lib --features fips -- crypto::fips
```

A failure in either step is a release blocker. See
`docs/runbooks/supply-chain.md § FIPS verification` for the
on-call procedure when a production pod reports
`crypto.substrate != ring-fips` against an environment where FIPS
is mandated.

## Doctrines

- **D14 fail-closed** — handshake refuses on any non-FIPS suite;
  CI fails on a broken `fips` build.
- **D19 reproducible** — the feature flag is a compile-time
  selection; the bytewise build output is deterministic per
  feature set.
- **D20 supply-chain SLSA L4** — `deny.toml § bans` refuses
  `openssl-sys` / `native-tls` so a transitive cannot smuggle a
  non-FIPS substrate into the graph.
- **D21 PQ agility** — the PQ-hybrid flag composes with FIPS
  cleanly; the cryptographic substrate is swappable without an API
  break.
