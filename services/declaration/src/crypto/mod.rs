// services/declaration/src/crypto/mod.rs
//
// TODO-051 / TODO-052 — FIPS-mode TLS + post-quantum hybrid KEX gate.
//
// This module is the single, testable surface that decides which
// cipher suites and key-exchange algorithms the service offers at
// TLS handshake. It is consumed by the rustls config builder in
// `infrastructure::tls` (production) and by the unit tests in this
// crate (CI matrix `build / fips-matrix`).
//
// Doctrines:
//   D14 fail-closed   — a mis-configured feature/env flag falls back
//                       to the SAFER posture (FIPS-only, classical-
//                       only KEX), not the weaker one. The fallback
//                       is logged at WARN.
//   D19 reproducible  — feature flags are compile-time selections;
//                       the resulting binary is deterministic per
//                       feature set.
//   D21 PQ agility    — pq_hybrid_kex_groups() is the single hook
//                       any future substrate swap (ML-KEM-1024,
//                       Kyber-AKS, etc.) flows through.
//
// The module is pure: no I/O, no clock, no env reads beyond what the
// caller passes in. Env-reading happens in `config.rs`.

pub mod fips;
pub mod pq_hybrid;

/// Cryptographic substrate identifier surfaced to the observability
/// stack at boot. Logged as `crypto.substrate=<value>` on startup so
/// on-call can confirm the deployed binary against
/// `docs/security/fips-mode.md § Verification`.
#[must_use]
pub fn substrate_identifier() -> &'static str {
    if cfg!(feature = "fips") {
        "ring-fips"
    } else {
        "ring-default"
    }
}
