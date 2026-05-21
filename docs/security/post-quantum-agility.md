# Post-quantum agility (TODO-052 / D21)

## Status

Shipped, **disabled-by-default**. PQ-hybrid is experimental in
upstream rustls 0.23. RÉCOR ships the wiring + the build flag now
so that when NIST finalises ML-KEM-768 in TLS 1.3 hybrid key
exchange (draft-ietf-tls-hybrid-design) and rustls promotes the
support out of `unstable-quic`, RÉCOR flips a config flag rather
than a code path.

## What this gates

`RECOR_PQ_HYBRID_ENABLED=true|false` (default `false`). When
`true` AND the binary was built with `--features pq-hybrid`, the
TLS layer's key-exchange algorithm list includes the hybrid suite
`X25519MLKEM768` ahead of the classical `X25519`. A peer that
supports the hybrid (or any peer that supports the IANA-registered
codepoint) negotiates the hybrid; a peer that does not supports
falls back to classical X25519 cleanly.

The two-gate design is deliberate:

1. **Compile-time gate** (`--features pq-hybrid`): the production
   image must be explicitly opted-in. Operators in environments
   that mandate FIPS-only key exchange (today, ML-KEM is not on the
   FIPS-140-3 approved-algorithm list) build without the feature.
2. **Runtime gate** (`RECOR_PQ_HYBRID_ENABLED=true`): even with
   the feature compiled in, the default-off posture means a
   misconfigured deployment cannot accidentally turn on the
   experimental KEX.

## Threat model addressed

Harvest-now-decrypt-later: a passive adversary records ciphertext
today and decrypts it once a cryptographically-relevant quantum
computer (CRQC) is available. RÉCOR's declaration submissions
carry beneficial-ownership data with a forty-year operational
relevance horizon; the harvest-now-decrypt-later window is
plausibly in scope.

The hybrid KEX ensures that for the confidentiality of TLS
session keys to be broken, *both* the classical (X25519) and the
post-quantum (ML-KEM-768) substrate must fall. ML-KEM-768 is a
Module-LWE construction; X25519 is an elliptic-curve construction;
the two substrates rest on disjoint hard-problem assumptions.

## How to enable

### Local build

```bash
cargo build --workspace --release --features pq-hybrid

RECOR_PQ_HYBRID_ENABLED=true \
  ./target/release/recor-declaration
```

### Container build

```bash
docker build \
  --build-arg CARGO_FEATURES=pq-hybrid \
  -f services/declaration/Dockerfile \
  -t recor/declaration:pq-hybrid \
  .
```

Compose with FIPS by setting `CARGO_FEATURES="fips pq-hybrid"`.

### Helm

In `infrastructure/helm/recor-service/values.yaml` (per-release
override):

```yaml
env:
  RECOR_PQ_HYBRID_ENABLED: "true"
```

The image tag must reference the `pq-hybrid`-flavoured build.

## Limitations

- **Experimental in rustls.** Upstream rustls < 0.24 does not yet
  export a stable PQ-hybrid surface; the feature gate today
  compiles into a no-op path that logs `pq.hybrid=requested
  pq.hybrid_active=false reason=upstream_unstable`. The wiring
  will become load-bearing on the rustls 0.24 bump (tracked as
  `R-PQ-RUSTLS-0.24`).
- **Not yet FIPS-approved.** Per `docs/security/fips-mode.md`
  § Limitations, combining `fips` + `pq-hybrid` is experimental and
  not yet appropriate for regulated FIPS-only environments.
- **Performance.** The ML-KEM-768 KEX adds ~1.1 KB to the
  ClientHello and ~1.2 KB to the ServerHello. RTT impact is
  negligible on first-handshake; session resumption is unchanged.
- **Backward compatibility.** Hybrid is offered alongside classical
  X25519; a client that does not understand the hybrid codepoint
  falls back. No traffic break.

## Verification

The unit test
`crypto::pq_hybrid::test_hybrid_kex_present_when_flag_set`
in each service's `src/crypto/pq_hybrid.rs` asserts that with
`pq-hybrid` compiled in AND `RECOR_PQ_HYBRID_ENABLED=true`, the
constructed rustls config includes the hybrid algorithm. With
either gate off, the config omits it.

```bash
cargo test --workspace --lib --features pq-hybrid -- pq_hybrid
```

A live handshake can be inspected with:

```bash
openssl s_client \
  -connect <service>:443 \
  -groups X25519MLKEM768:X25519 \
  -tls1_3 < /dev/null 2>&1 | grep 'Server Temp Key'
```

A server that negotiated the hybrid prints a Server Temp Key line
containing the algorithm identifier `X25519MLKEM768`.

## Migration plan

| Stage | Trigger | Action |
|-------|---------|--------|
| 1 | Today | Ship feature flag + config gate; default-off. |
| 2 | rustls 0.24 stable + ML-KEM ratified in TLS 1.3 | Promote the wiring out of `cfg(feature = "pq-hybrid")`. |
| 3 | CRQC-imminent signal (NSA / NIST advisory) | Flip `RECOR_PQ_HYBRID_ENABLED` to default-true; deprecate classical-only handshakes. |
| 4 | Cryptanalytic break of ML-KEM-768 | Pivot to ML-KEM-1024 (D21 substrate). |

## Doctrines

- **D14 fail-closed** — an unsupported hybrid configuration logs
  loudly and falls back to classical; the service does not silently
  serve a weaker key exchange than configured.
- **D21 post-quantum agility** — substrate swap is a feature-flag
  combined with a config-flag change, not a code rewrite.
- **D19 reproducible** — the feature flag is compile-time;
  per-flag build outputs are bytewise deterministic.
