// services/declaration/src/crypto/fips.rs
//
// TODO-051. FIPS-approved cipher-suite list for the rustls TLS layer.
// When the crate is compiled with `--features fips`, the consumer
// (`infrastructure::tls`) passes `fips_only_suite_names()` into the
// rustls config builder; the resulting handshake will refuse any
// suite outside this list.
//
// The list is the FIPS-140-2 Level 1 approved set under the `ring`
// substrate, documented at `docs/security/fips-mode.md`.

/// The FIPS-approved TLS 1.3 + TLS 1.2 cipher suites we offer when
/// `--features fips` is on. ChaCha20-Poly1305 is intentionally
/// excluded; TLS 1.0 / 1.1 / SSLv3 are not represented at all (the
/// upstream rustls only supports 1.2 and 1.3).
pub const FIPS_APPROVED_SUITE_NAMES: &[&str] = &[
    // TLS 1.3
    "TLS_AES_256_GCM_SHA384",
    "TLS_AES_128_GCM_SHA256",
    // TLS 1.2 — ECDSA
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    // TLS 1.2 — RSA
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
];

/// Returns the FIPS-approved suite-name allow-list when the
/// `fips` feature is compiled in; an empty slice otherwise (the
/// caller defaults to the rustls library's safe-default suite list).
#[must_use]
pub fn fips_only_suite_names() -> &'static [&'static str] {
    if cfg!(feature = "fips") {
        FIPS_APPROVED_SUITE_NAMES
    } else {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fips_suite_list_excludes_chacha20() {
        // ChaCha20-Poly1305 is not on the FIPS-approved AEAD list;
        // the suite-name allow-list must not contain it.
        for suite in FIPS_APPROVED_SUITE_NAMES {
            assert!(
                !suite.contains("CHACHA20"),
                "FIPS allow-list must not include ChaCha20: {suite}"
            );
        }
    }

    #[test]
    fn fips_suite_list_only_uses_gcm_aead() {
        for suite in FIPS_APPROVED_SUITE_NAMES {
            // TLS 1.3 names omit the explicit AEAD prefix (they
            // imply GCM); TLS 1.2 names include `_GCM_`.
            let ok = suite.starts_with("TLS_AES_")
                || suite.contains("_GCM_");
            assert!(ok, "FIPS allow-list must use GCM AEAD: {suite}");
        }
    }

    #[cfg(feature = "fips")]
    #[test]
    fn fips_feature_returns_nonempty_allowlist() {
        assert!(!fips_only_suite_names().is_empty());
    }

    #[cfg(not(feature = "fips"))]
    #[test]
    fn default_feature_returns_empty_allowlist() {
        // Without the feature, the caller falls back to rustls's
        // own safe-default suites — fips_only_suite_names returns
        // empty to signal "no FIPS-specific override".
        assert!(fips_only_suite_names().is_empty());
    }
}
