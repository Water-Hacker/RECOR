//! `recor-hmac-sig` — shared HMAC-SHA256 signing + verification with an
//! `iat`-bound replay window.
//!
//! Closes audit FIND-012. The pre-fix verify path computed
//! `HMAC(secret, body)`; a captured envelope could be replayed
//! indefinitely until the secret rotated. The new path computes
//! `HMAC(secret, body || "\n" || iat_seconds)` and refuses every
//! request whose timestamp falls outside a ±N-second window from
//! the receiver's clock.
//!
//! Wire contract:
//!
//!   - Producer side: emit two headers per request:
//!       `X-RECOR-Signature: <hex(mac)>`
//!       `X-RECOR-Timestamp: <unix_seconds>`
//!   - Receiver side: read both headers, compute the same MAC, and
//!     compare in constant time.
//!
//! Rotation: callers pass an `Option<&str>` for the previous-generation
//! secret. When provided, verification succeeds against either secret.
//! Mirrors the ADR-005 dual-secret rotation pattern already in use.
//!
//! Window: defaults to 300 seconds (5 minutes) on either side of the
//! receiver's clock. Receivers can override via [`VerifyConfig::window_seconds`].
//! D14 fail-closed: a missing timestamp, malformed timestamp, or
//! window violation is `VerifyError`; the caller maps it to 401.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;
use time::OffsetDateTime;

type HmacSha256 = Hmac<Sha256>;

/// Sign a request body. Returns the hex-encoded MAC. The caller is
/// expected to set both the signature header AND the timestamp
/// header on the outbound request.
pub fn sign(secret: &str, body: &[u8], iat_unix_seconds: i64) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    mac.update(b"\n");
    mac.update(iat_unix_seconds.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Tunable knobs for the verifier.
#[derive(Debug, Clone)]
pub struct VerifyConfig<'a> {
    /// Current-generation secret. REQUIRED.
    pub primary_secret: &'a str,
    /// Previous-generation secret. `None` ⇒ rotation not in progress.
    pub old_secret: Option<&'a str>,
    /// Replay-window half-width, seconds. Defaults to 300 if you use
    /// [`VerifyConfig::primary`].
    pub window_seconds: u64,
}

impl<'a> VerifyConfig<'a> {
    /// Construct a config with the default 300-second window and no
    /// rotation secret.
    pub fn primary(primary_secret: &'a str) -> Self {
        Self {
            primary_secret,
            old_secret: None,
            window_seconds: 300,
        }
    }

    pub fn with_old_secret(mut self, old: &'a str) -> Self {
        self.old_secret = Some(old);
        self
    }

    pub fn with_window_seconds(mut self, w: u64) -> Self {
        self.window_seconds = w;
        self
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("X-RECOR-Timestamp header missing")]
    TimestampMissing,
    #[error("X-RECOR-Timestamp value is not a valid unix-seconds integer")]
    TimestampMalformed,
    #[error("request timestamp is outside the replay window (drift {drift_seconds}s, max ±{window_seconds}s)")]
    OutsideWindow { drift_seconds: i64, window_seconds: u64 },
    #[error("X-RECOR-Signature header missing")]
    SignatureMissing,
    #[error("X-RECOR-Signature is not valid hex")]
    SignatureMalformed,
    #[error("HMAC signature did not verify against the configured secret(s)")]
    BadSignature,
}

/// Verify a request body against a signature header + timestamp header.
///
/// `signature_hex` and `timestamp_str` are passed as `Option<&str>`
/// because the call site typically reads them from a `HeaderMap` and
/// gets back `None` if the header is absent. Both must be `Some` to
/// proceed; missing or malformed is `VerifyError`.
///
/// `now` lets tests inject a deterministic clock. Production callers
/// should pass `OffsetDateTime::now_utc().unix_timestamp()`.
pub fn verify(
    cfg: &VerifyConfig<'_>,
    body: &[u8],
    signature_hex: Option<&str>,
    timestamp_str: Option<&str>,
    now_unix_seconds: i64,
) -> Result<(), VerifyError> {
    // Window check FIRST. A request with a missing/stale timestamp is
    // rejected before the HMAC compare so a forged signature can't
    // burn a CPU cycle on the receiver.
    let ts_str = timestamp_str.ok_or(VerifyError::TimestampMissing)?;
    let ts: i64 = ts_str
        .trim()
        .parse()
        .map_err(|_| VerifyError::TimestampMalformed)?;
    let drift = now_unix_seconds - ts;
    let window = cfg.window_seconds as i64;
    if drift.abs() > window {
        return Err(VerifyError::OutsideWindow {
            drift_seconds: drift,
            window_seconds: cfg.window_seconds,
        });
    }

    let sig_hex = signature_hex.ok_or(VerifyError::SignatureMissing)?;
    let provided =
        hex::decode(sig_hex.trim()).map_err(|_| VerifyError::SignatureMalformed)?;

    // Try primary; if rotation is in progress, fall back to old.
    if verify_against(cfg.primary_secret, body, ts, &provided) {
        return Ok(());
    }
    if let Some(old) = cfg.old_secret {
        if !old.is_empty() && verify_against(old, body, ts, &provided) {
            return Ok(());
        }
    }
    Err(VerifyError::BadSignature)
}

fn verify_against(secret: &str, body: &[u8], iat: i64, provided: &[u8]) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.update(b"\n");
    mac.update(iat.to_string().as_bytes());
    mac.verify_slice(provided).is_ok()
}

/// Convenience helper for callers that want a wall-clock now value
/// without pulling in `time` themselves.
pub fn now_unix_seconds() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-do-not-use-in-prod";

    #[test]
    fn round_trip_signs_and_verifies() {
        let body = b"hello world";
        let iat = 1_700_000_000;
        let sig = sign(SECRET, body, iat);
        let cfg = VerifyConfig::primary(SECRET);
        verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat)
            .expect("fresh signature must verify");
    }

    #[test]
    fn missing_timestamp_header_fails_closed() {
        let body = b"hello";
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, body, Some("ff"), None, 0).unwrap_err();
        assert_eq!(err, VerifyError::TimestampMissing);
    }

    #[test]
    fn missing_signature_header_fails_closed() {
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, b"x", None, Some("0"), 0).unwrap_err();
        assert_eq!(err, VerifyError::SignatureMissing);
    }

    #[test]
    fn stale_timestamp_outside_window_fails_closed() {
        let body = b"hello";
        let iat = 1_700_000_000;
        let sig = sign(SECRET, body, iat);
        let cfg = VerifyConfig::primary(SECRET).with_window_seconds(60);
        // Receiver clock is 200s ahead — outside the 60s window.
        let err = verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat + 200)
            .unwrap_err();
        assert!(matches!(err, VerifyError::OutsideWindow { .. }));
    }

    #[test]
    fn future_dated_timestamp_outside_window_fails_closed() {
        let body = b"hello";
        let iat = 1_700_000_000;
        let sig = sign(SECRET, body, iat);
        let cfg = VerifyConfig::primary(SECRET).with_window_seconds(60);
        // Receiver clock is 200s behind — signature appears 200s in the future.
        let err = verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat - 200)
            .unwrap_err();
        assert!(matches!(err, VerifyError::OutsideWindow { .. }));
    }

    #[test]
    fn wrong_signature_fails_closed() {
        let body = b"hello";
        let iat = 1_700_000_000;
        // Sign with one secret, verify with another.
        let sig = sign("other-secret", body, iat);
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat)
            .unwrap_err();
        assert_eq!(err, VerifyError::BadSignature);
    }

    #[test]
    fn body_tampering_invalidates_signature() {
        let iat = 1_700_000_000;
        let sig = sign(SECRET, b"original", iat);
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, b"tampered", Some(&sig), Some(&iat.to_string()), iat)
            .unwrap_err();
        assert_eq!(err, VerifyError::BadSignature);
    }

    #[test]
    fn timestamp_tampering_invalidates_signature() {
        let iat = 1_700_000_000;
        let sig = sign(SECRET, b"hello", iat);
        let cfg = VerifyConfig::primary(SECRET);
        // Send a tampered timestamp (within window) — the MAC bound to
        // the original iat won't verify against the new iat.
        let err = verify(&cfg, b"hello", Some(&sig), Some(&(iat + 1).to_string()), iat + 1)
            .unwrap_err();
        assert_eq!(err, VerifyError::BadSignature);
    }

    #[test]
    fn rotation_old_secret_accepted_when_configured() {
        let body = b"hello";
        let iat = 1_700_000_000;
        let old = "old-secret";
        let sig = sign(old, body, iat);
        let cfg = VerifyConfig::primary(SECRET).with_old_secret(old);
        verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat)
            .expect("signature under old secret must verify during rotation");
    }

    #[test]
    fn rotation_empty_old_secret_does_not_match() {
        let body = b"hello";
        let iat = 1_700_000_000;
        let sig = sign("", body, iat);
        let cfg = VerifyConfig::primary(SECRET).with_old_secret("");
        // Empty old_secret must NOT match anything — guards against a
        // misconfigured rotation flag silently accepting MACs signed
        // with the empty key.
        let err = verify(&cfg, body, Some(&sig), Some(&iat.to_string()), iat)
            .unwrap_err();
        assert_eq!(err, VerifyError::BadSignature);
    }

    #[test]
    fn malformed_timestamp_fails_closed() {
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, b"x", Some("ff"), Some("not-a-number"), 0).unwrap_err();
        assert_eq!(err, VerifyError::TimestampMalformed);
    }

    #[test]
    fn malformed_signature_hex_fails_closed() {
        let cfg = VerifyConfig::primary(SECRET);
        let err = verify(&cfg, b"x", Some("zz"), Some("0"), 0).unwrap_err();
        assert_eq!(err, VerifyError::SignatureMalformed);
    }
}
