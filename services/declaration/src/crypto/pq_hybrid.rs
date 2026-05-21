// services/declaration/src/crypto/pq_hybrid.rs
//
// TODO-052 / D21. Post-quantum hybrid key-exchange gate for the
// rustls TLS layer.
//
// Two-gate activation (see docs/security/post-quantum-agility.md):
//   1. Compile-time feature flag `pq-hybrid` (this module is built).
//   2. Runtime config flag `RECOR_PQ_HYBRID_ENABLED=true` (the
//      consumer of `pq_hybrid_kex_groups` reads this).
//
// When both gates are open, the service's rustls config offers the
// hybrid suite ahead of classical X25519. A peer that does not
// understand the hybrid codepoint falls back to classical, so the
// posture is forward-compatible.

/// Symbolic identifier for the X25519 + ML-KEM-768 hybrid KEX,
/// per IANA TLS named-group registry (codepoint 0x11EC,
/// X25519MLKEM768).
pub const HYBRID_GROUP_NAME: &str = "X25519MLKEM768";

/// Symbolic identifier for classical X25519, always offered as the
/// fallback.
pub const CLASSICAL_GROUP_NAME: &str = "X25519";

/// Returns the ordered list of key-exchange group names to offer at
/// handshake.
///
/// - `pq_hybrid_enabled = true`  AND `cfg(feature = "pq-hybrid")` ⇒
///   `[X25519MLKEM768, X25519]` — hybrid preferred, classical
///   fallback retained for non-PQ peers.
/// - any other configuration ⇒ `[X25519]` — classical only.
///
/// The function is pure; the config-flag read happens in the caller
/// (`config.rs`) to keep this module unit-testable without env
/// scaffolding.
#[must_use]
pub fn pq_hybrid_kex_groups(pq_hybrid_enabled: bool) -> Vec<&'static str> {
    if pq_hybrid_enabled && cfg!(feature = "pq-hybrid") {
        vec![HYBRID_GROUP_NAME, CLASSICAL_GROUP_NAME]
    } else {
        vec![CLASSICAL_GROUP_NAME]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classical_only_when_runtime_disabled() {
        // Runtime gate off ⇒ classical-only regardless of feature.
        let groups = pq_hybrid_kex_groups(false);
        assert_eq!(groups, vec![CLASSICAL_GROUP_NAME]);
        assert!(!groups.contains(&HYBRID_GROUP_NAME));
    }

    #[cfg(feature = "pq-hybrid")]
    #[test]
    fn test_hybrid_kex_present_when_flag_set() {
        // Smoke test referenced in docs/security/post-quantum-
        // agility.md § Verification. With the feature compiled in
        // AND the runtime gate on, the hybrid group MUST appear
        // ahead of classical.
        let groups = pq_hybrid_kex_groups(true);
        assert_eq!(groups.first(), Some(&HYBRID_GROUP_NAME));
        assert!(groups.contains(&CLASSICAL_GROUP_NAME));
    }

    #[cfg(not(feature = "pq-hybrid"))]
    #[test]
    fn hybrid_absent_when_feature_disabled() {
        // Feature off ⇒ classical-only even if the runtime gate is on.
        let groups = pq_hybrid_kex_groups(true);
        assert_eq!(groups, vec![CLASSICAL_GROUP_NAME]);
    }
}
