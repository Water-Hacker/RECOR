//! OIDC Bearer-token verification.
//!
//! Production-grade JWT verification for the protected REST surface:
//! discovers the issuer's JWKS via `.well-known/openid-configuration`,
//! caches the JWKS with a TTL, and verifies signature + `iss` + `aud`
//! + `exp` + `nbf` on every request.
//!
//! Supported algorithms: RS256, RS384, RS512, ES256, ES384, EdDSA. The
//! algorithm is picked from the token header's `alg` field; the JWKS
//! key is matched by `kid`. Algorithm-confusion attacks (e.g. swapping
//! RS256 for HS256) are refused because we never accept HMAC algorithms.
//!
//! D14 (fail-closed): every error path returns `VerificationError`,
//! which the auth middleware maps to 401. There is no fallback to an
//! unverified path. The empty-issuer path (R-DECL-1's predecessor) is
//! refused at config load when `environment != "dev"`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{
    decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, instrument, warn};

/// Subset of the JWT claims we read. Additional claims pass through
/// untouched in the underlying validation; we only require what the
/// principal-resolution path needs.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    /// Single-audience tokens omit this in some providers (Keycloak v18+);
    /// multi-audience tokens populate it. `Validation` enforces audience
    /// before this struct is materialised.
    #[serde(default)]
    pub aud: serde_json::Value,
    pub exp: i64,
    #[serde(default)]
    pub nbf: Option<i64>,
    #[serde(default)]
    pub iat: Option<i64>,
}

/// OIDC verifier — holds the issuer config, JWKS cache, and HTTP client.
/// `Arc<Self>` is shared across all request handlers via the auth state.
pub struct OidcVerifier {
    issuer: String,
    audience: String,
    /// JWKS URL discovered from `.well-known/openid-configuration` at
    /// construction time, or set directly if the caller knows it.
    jwks_url: String,
    /// Leeway in seconds for `exp`, `nbf`, and `iat` to absorb clock
    /// skew between issuer and verifier.
    leeway_seconds: u64,
    /// Cache TTL — refetch the JWKS at most every `cache_ttl`.
    cache_ttl: Duration,
    http: reqwest::Client,
    cache: RwLock<JwksCache>,
}

#[derive(Default)]
struct JwksCache {
    jwks: Option<JwkSet>,
    fetched_at: Option<Instant>,
}

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("discovery failed at {url}: {source}")]
    DiscoveryFailed {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("JWKS fetch failed at {url}: {source}")]
    JwksFetchFailed {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("missing kid in token header")]
    MissingKid,

    #[error("no JWKS key matches kid {0}")]
    UnknownKid(String),

    #[error("unsupported algorithm {0:?}")]
    UnsupportedAlgorithm(Algorithm),

    #[error("JWKS does not include any supported asymmetric key")]
    NoUsableKey,

    #[error("token decode/verify failed: {0}")]
    TokenInvalid(#[from] jsonwebtoken::errors::Error),

    #[error("malformed token header")]
    MalformedHeader,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscoveryDocument {
    jwks_uri: String,
}

impl OidcVerifier {
    /// Construct a verifier by discovering the JWKS endpoint from the
    /// issuer's OIDC discovery document. The issuer string MUST match
    /// the `iss` claim on every token verified — that's the OIDC
    /// integrity property.
    pub async fn discover(
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Result<Arc<Self>, VerificationError> {
        let issuer = issuer.into();
        let audience = audience.into();
        Self::discover_inner(issuer, audience).await
    }

    #[instrument(skip_all, fields(issuer = %issuer, audience = %audience))]
    async fn discover_inner(
        issuer: String,
        audience: String,
    ) -> Result<Arc<Self>, VerificationError> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client builds");
        let doc: DiscoveryDocument = http
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| VerificationError::DiscoveryFailed {
                url: discovery_url.clone(),
                source: e,
            })?
            .error_for_status()
            .map_err(|e| VerificationError::DiscoveryFailed {
                url: discovery_url.clone(),
                source: e,
            })?
            .json()
            .await
            .map_err(|e| VerificationError::DiscoveryFailed {
                url: discovery_url,
                source: e,
            })?;
        Ok(Arc::new(Self {
            issuer,
            audience,
            jwks_url: doc.jwks_uri,
            leeway_seconds: 30,
            cache_ttl: Duration::from_secs(300),
            http,
            cache: RwLock::new(JwksCache::default()),
        }))
    }

    /// Construct a verifier with an explicit JWKS URL — used in tests
    /// and in environments where the issuer does not expose an OIDC
    /// discovery document. The caller is responsible for setting
    /// `issuer` to the value the tokens will carry in their `iss`
    /// claim.
    pub fn with_jwks_url(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        jwks_url: impl Into<String>,
    ) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client builds");
        Arc::new(Self {
            issuer: issuer.into(),
            audience: audience.into(),
            jwks_url: jwks_url.into(),
            leeway_seconds: 30,
            cache_ttl: Duration::from_secs(300),
            http,
            cache: RwLock::new(JwksCache::default()),
        })
    }

    /// Verify a Bearer token's signature + standard claims, returning
    /// the decoded claims on success.
    #[instrument(skip_all)]
    pub async fn verify(&self, token: &str) -> Result<Claims, VerificationError> {
        let header = decode_header(token).map_err(|_| VerificationError::MalformedHeader)?;
        let alg = supported_alg(header.alg)?;
        let kid = header.kid.ok_or(VerificationError::MissingKid)?;

        let key = self.lookup_key(&kid).await?;

        let mut validation = Validation::new(alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.leeway = self.leeway_seconds;
        validation.validate_exp = true;
        validation.validate_nbf = true;

        let data = decode::<Claims>(token, &key, &validation)?;
        Ok(data.claims)
    }

    /// Look up a key by `kid`. Fetches the JWKS on cache miss or expiry.
    async fn lookup_key(&self, kid: &str) -> Result<DecodingKey, VerificationError> {
        if let Some(key) = self.lookup_cached(kid).await? {
            return Ok(key);
        }
        // Cache miss or stale — refetch.
        self.refresh_jwks().await?;
        self.lookup_cached(kid)
            .await?
            .ok_or_else(|| VerificationError::UnknownKid(kid.to_string()))
    }

    async fn lookup_cached(
        &self,
        kid: &str,
    ) -> Result<Option<DecodingKey>, VerificationError> {
        let cache = self.cache.read().await;
        if !is_fresh(&cache, self.cache_ttl) {
            return Ok(None);
        }
        let Some(set) = cache.jwks.as_ref() else {
            return Ok(None);
        };
        match set.find(kid) {
            None => Ok(None),
            Some(jwk) => DecodingKey::from_jwk(jwk)
                .map(Some)
                .map_err(VerificationError::TokenInvalid),
        }
    }

    async fn refresh_jwks(&self) -> Result<(), VerificationError> {
        debug!(jwks_url = %self.jwks_url, "fetching JWKS");
        let set: JwkSet = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| VerificationError::JwksFetchFailed {
                url: self.jwks_url.clone(),
                source: e,
            })?
            .error_for_status()
            .map_err(|e| VerificationError::JwksFetchFailed {
                url: self.jwks_url.clone(),
                source: e,
            })?
            .json()
            .await
            .map_err(|e| VerificationError::JwksFetchFailed {
                url: self.jwks_url.clone(),
                source: e,
            })?;
        if set.keys.is_empty() {
            warn!("JWKS returned with zero keys");
            return Err(VerificationError::NoUsableKey);
        }
        let mut cache = self.cache.write().await;
        cache.jwks = Some(set);
        cache.fetched_at = Some(Instant::now());
        Ok(())
    }
}

fn is_fresh(cache: &JwksCache, ttl: Duration) -> bool {
    match cache.fetched_at {
        None => false,
        Some(t) => t.elapsed() < ttl,
    }
}

/// Refuse HMAC algorithms outright — algorithm confusion attacks rely
/// on the verifier accepting HS256 with the public key bytes treated
/// as an HMAC secret. We accept only asymmetric algorithms.
fn supported_alg(alg: Algorithm) -> Result<Algorithm, VerificationError> {
    match alg {
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::ES256
        | Algorithm::ES384
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512
        | Algorithm::EdDSA => Ok(alg),
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            Err(VerificationError::UnsupportedAlgorithm(alg))
        }
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use time::OffsetDateTime;

    use super::*;

    const TEST_ISSUER: &str = "https://issuer.test";
    const TEST_AUDIENCE: &str = "recor-test-aud";
    const TEST_KID: &str = "test-key-1";

    fn rs256_keypair() -> (EncodingKey, jsonwebtoken::jwk::Jwk) {
        // Static PKCS#8 RSA-2048 keypair generated once and pinned for
        // deterministic tests. Generated with:
        //   openssl genpkey -algorithm RSA -pkcs8 -out test_rsa.pem -outform PEM \
        //                   -pkeyopt rsa_keygen_bits:2048
        //   openssl rsa -in test_rsa.pem -pubout
        //
        // These keys are TEST-ONLY and have never been used elsewhere.
        const PEM: &str = include_str!("../../tests/fixtures/test_rsa_pkcs8.pem");
        let encoding =
            EncodingKey::from_rsa_pem(PEM.as_bytes()).expect("test RSA PEM is valid");
        let jwk_json = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../tests/fixtures/test_rsa_jwk.json"
        ))
        .expect("test JWK is valid JSON");
        let jwk: jsonwebtoken::jwk::Jwk =
            serde_json::from_value(jwk_json).expect("test JWK matches schema");
        (encoding, jwk)
    }

    fn sign(claims: serde_json::Value, key: &EncodingKey) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        encode(&header, &claims, key).expect("encode")
    }

    fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    fn make_verifier(jwks: JwkSet) -> Arc<OidcVerifier> {
        // Build a verifier with an empty URL but a pre-seeded cache so
        // we exercise the verification path without an HTTP round-trip.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        Arc::new(OidcVerifier {
            issuer: TEST_ISSUER.to_string(),
            audience: TEST_AUDIENCE.to_string(),
            jwks_url: "http://127.0.0.1:0/jwks-unreachable".to_string(),
            leeway_seconds: 30,
            cache_ttl: Duration::from_secs(60),
            http,
            cache: RwLock::new(JwksCache {
                jwks: Some(jwks),
                fetched_at: Some(Instant::now()),
            }),
        })
    }

    #[tokio::test]
    async fn valid_token_verifies() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "spiffe://recor.cm/declarant-1",
                "exp": now() + 300,
                "nbf": now() - 10,
                "iat": now() - 10,
            }),
            &signing,
        );
        let claims = v.verify(&token).await.unwrap();
        assert_eq!(claims.sub, "spiffe://recor.cm/declarant-1");
    }

    #[tokio::test]
    async fn expired_token_rejects() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() - 3600, // 1 hour ago
            }),
            &signing,
        );
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, VerificationError::TokenInvalid(_)));
    }

    #[tokio::test]
    async fn wrong_issuer_rejects() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": "https://attacker.example",
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 300,
            }),
            &signing,
        );
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, VerificationError::TokenInvalid(_)));
    }

    #[tokio::test]
    async fn wrong_audience_rejects() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": "different-audience",
                "sub": "x",
                "exp": now() + 300,
            }),
            &signing,
        );
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, VerificationError::TokenInvalid(_)));
    }

    #[tokio::test]
    async fn unknown_kid_rejects_after_refresh_fails() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("does-not-exist".to_string());
        let token = encode(
            &header,
            &json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 300,
            }),
            &signing,
        )
        .unwrap();
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(
            err,
            VerificationError::JwksFetchFailed { .. } | VerificationError::UnknownKid(_)
        ));
    }

    #[tokio::test]
    async fn missing_kid_rejects() {
        let (signing, _) = rs256_keypair();
        // Encode with no kid header — jsonwebtoken's default Header has
        // no kid unless you set it.
        let header = Header::new(Algorithm::RS256);
        let token = encode(
            &header,
            &json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 300,
            }),
            &signing,
        )
        .unwrap();
        let v = make_verifier(JwkSet { keys: vec![] });
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, VerificationError::MissingKid));
    }

    #[tokio::test]
    async fn malformed_token_rejects() {
        let v = make_verifier(JwkSet { keys: vec![] });
        let err = v.verify("not.a.jwt").await.unwrap_err();
        assert!(matches!(err, VerificationError::MalformedHeader));
    }

    #[test]
    fn hmac_algorithms_are_refused() {
        assert!(matches!(
            supported_alg(Algorithm::HS256),
            Err(VerificationError::UnsupportedAlgorithm(_))
        ));
        assert!(matches!(
            supported_alg(Algorithm::HS512),
            Err(VerificationError::UnsupportedAlgorithm(_))
        ));
    }
}
