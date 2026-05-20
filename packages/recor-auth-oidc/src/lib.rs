//! `recor-auth-oidc` — production-grade OIDC Bearer-token verification.
//!
//! Closes R-AUTH-1 (#46). Single shared implementation used by both
//! `services/declaration` and `services/verification-engine`. Replaces
//! the previously-duplicated `src/api/oidc.rs` modules in each service.
//!
//! Discovers the issuer's JWKS via `.well-known/openid-configuration`,
//! caches the JWKS with a TTL, and verifies signature + `iss` + `aud`
//! + `exp` + `nbf` on every request.
//!
//! Supported algorithms: RS256, RS384, RS512, ES256, ES384, EdDSA. The
//! algorithm is picked from the token header's `alg` field; the JWKS
//! key is matched by `kid`. Algorithm-confusion attacks (e.g. swapping
//! RS256 for HS256) are refused because we never accept HMAC algorithms.
//!
//! Hardening (R-AUTH-2 / R-AUTH-3 / R-AUTH-4):
//!   - **Subject-claim mapping** — the principal subject is extracted
//!     from a configurable claim (default `sub`). Some issuers prefer
//!     `preferred_username`, `email`, or a custom claim.
//!   - **JWKS pre-warm** — `discover()` fetches the JWKS once at
//!     startup so the first request doesn't pay a round-trip to the
//!     issuer. Discovery failure refuses to start.
//!   - **Verified-token LRU cache** — successful verifications are
//!     memoised by raw token string; cache entries expire at the
//!     token's `exp`. A token presented in a tight loop verifies once,
//!     then hits the cache. Bounded size (default 1024); revocation
//!     follows expiry semantics, not a separate signal.
//!
//! D14 (fail-closed): every error path returns `VerificationError`,
//! which the auth middleware maps to 401. There is no fallback to an
//! unverified path. The empty-issuer path is refused at config load
//! when `environment != "dev"`.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{
    decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation,
};
use lru::LruCache;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, instrument, warn};

/// Subset of the JWT claims we surface to the application layer.
/// `sub` is the resolved principal subject — pulled from whichever
/// claim the verifier was configured to read (default `"sub"`).
#[derive(Debug, Clone)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub exp: i64,
    /// The full JWT payload, available for handlers that need to read
    /// custom claims (scopes, roles, tenant). Validation has already
    /// passed; consumers can trust the contents.
    pub raw: JsonValue,
}

/// Number of verified-token entries to keep in the LRU.
const DEFAULT_TOKEN_CACHE_CAP: usize = 1024;

/// OIDC verifier — holds the issuer config, JWKS cache, verified-token
/// LRU, and HTTP client. `Arc<Self>` is shared across all request
/// handlers via the auth state.
pub struct OidcVerifier {
    issuer: String,
    audience: String,
    /// JWKS URL discovered from `.well-known/openid-configuration` at
    /// construction time, or set directly if the caller knows it.
    jwks_url: String,
    /// Name of the claim that becomes the Principal's subject. Default
    /// `"sub"`; production may want `"preferred_username"`, `"email"`,
    /// or a custom claim.
    subject_claim: String,
    /// Leeway in seconds for `exp`, `nbf`, and `iat` to absorb clock
    /// skew between issuer and verifier.
    leeway_seconds: u64,
    /// Cache TTL — refetch the JWKS at most every `cache_ttl`.
    cache_ttl: Duration,
    http: reqwest::Client,
    cache: RwLock<JwksCache>,
    /// Verified-token cache. Key = raw token string; value = (Claims,
    /// token's `exp` as unix seconds). A miss verifies the signature
    /// and inserts; a hit on a still-valid entry returns immediately.
    /// `Mutex` (not RwLock) because `lru::LruCache::get` mutates the
    /// recency list.
    token_cache: Mutex<LruCache<String, (Claims, i64)>>,
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

    #[error("token missing required claim {0}")]
    MissingClaim(&'static str),

    #[error("subject claim {claim} not present or not a string in token")]
    SubjectClaimAbsent { claim: String },

    /// TODO-020 / FIND-020 — the verified token's ACR claim resolves to
    /// an assurance level lower than the endpoint's configured minimum.
    /// Maps to `AuthError::AuthenticationRequired` (401) at the HTTP
    /// boundary; the body explains the policy deliberately (so a
    /// supervisor's operator can diagnose) but never echoes the
    /// presented claim back (no log/header injection surface).
    #[error("insufficient assurance level: token presents `{presented}`, endpoint requires {required:?}")]
    InsufficientAssurance {
        presented: String,
        required: AssuranceLevel,
    },
}

/// NIST 800-63A IAL (Identity Assurance Level) / 800-63B AAL
/// (Authentication Assurance Level) ladder.
///
/// The platform treats them as a single monotonic ladder for endpoint-
/// minimum enforcement; the runbook
/// `docs/runbooks/oidc-idp-acr-config.md` documents the per-IdP
/// configuration that maps an issuer's policy to this ladder.
///
/// Per FATF c.24.6 IO.5 ("identity verification of the submitter"):
///   - Read-only endpoints are admissible at IAL1 (self-asserted).
///   - State-changing submission endpoints require IAL2 (verified
///     evidence + verified address).
///   - Administrative endpoints (dissolve, correct, merge-into,
///     dlq/replay) require IAL3 (in-person or supervised remote
///     verification).
///
/// The IdP's `acr` claim is parsed via [`AssuranceLevel::from_acr_claim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssuranceLevel {
    /// IAL1 — self-asserted identity. The fail-closed default when the
    /// IdP does not advertise an `acr` claim.
    Ial1,
    /// IAL2 — verified evidence + verified address. The platform-wide
    /// default minimum for state-changing endpoints (TODO-020).
    Ial2,
    /// IAL3 — in-person or supervised remote verification. Required for
    /// administrative endpoints.
    Ial3,
}

impl AssuranceLevel {
    /// Parse the OIDC `acr` claim into an [`AssuranceLevel`]. Accepts
    /// the three canonical forms operators in the wild use:
    ///
    /// 1. The NIST URIs:
    ///    `http://idmanagement.gov/ns/assurance/ial/{1,2,3}` and
    ///    `https://refeds.org/profile/sfa` / similar.
    /// 2. The bare numeric levels `"1" | "2" | "3"` (Keycloak / Auth0
    ///    style; many corporate IdPs follow ISO/IEC 29115).
    /// 3. The case-insensitive `"ial1" | "ial2" | "ial3"`.
    ///
    /// Any unrecognised value resolves to **IAL1** — the fail-closed
    /// floor — and the caller is expected to refuse if the endpoint's
    /// minimum is anything stricter (D14).
    pub fn from_acr_claim(value: &str) -> Self {
        let trimmed = value.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.ends_with("/ial/3") || lower == "ial3" || lower == "3" {
            AssuranceLevel::Ial3
        } else if lower.ends_with("/ial/2") || lower == "ial2" || lower == "2" {
            AssuranceLevel::Ial2
        } else if lower.ends_with("/ial/1") || lower == "ial1" || lower == "1" || lower == "0" {
            AssuranceLevel::Ial1
        } else {
            AssuranceLevel::Ial1
        }
    }

    /// Wire-friendly name (for logs / matrix docs).
    pub fn as_str(&self) -> &'static str {
        match self {
            AssuranceLevel::Ial1 => "IAL1",
            AssuranceLevel::Ial2 => "IAL2",
            AssuranceLevel::Ial3 => "IAL3",
        }
    }
}

impl Claims {
    /// TODO-020 — Extract the OIDC `acr` claim and resolve it to an
    /// [`AssuranceLevel`]. Absent / non-string → `Ial1` (the fail-
    /// closed floor; the caller must enforce its endpoint minimum
    /// against this).
    pub fn assurance_level(&self) -> AssuranceLevel {
        self.raw
            .get("acr")
            .and_then(|v| v.as_str())
            .map(AssuranceLevel::from_acr_claim)
            .unwrap_or(AssuranceLevel::Ial1)
    }

    /// Enforce a per-endpoint minimum assurance level. Returns
    /// [`VerificationError::InsufficientAssurance`] when the token's
    /// ACR resolves below `min`.
    pub fn enforce_assurance(&self, min: AssuranceLevel) -> Result<(), VerificationError> {
        let presented_level = self.assurance_level();
        if presented_level >= min {
            Ok(())
        } else {
            let presented = self
                .raw
                .get("acr")
                .and_then(|v| v.as_str())
                .unwrap_or("<absent>")
                .to_string();
            Err(VerificationError::InsufficientAssurance {
                presented,
                required: min,
            })
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DiscoveryDocument {
    jwks_uri: String,
}

/// Builder-style configuration for the verifier. Mostly used by tests
/// and bespoke deployments; production paths use `discover()`.
pub struct OidcVerifierBuilder {
    issuer: String,
    audience: String,
    jwks_url: Option<String>,
    subject_claim: String,
    leeway_seconds: u64,
    cache_ttl: Duration,
    token_cache_capacity: NonZeroUsize,
}

impl OidcVerifierBuilder {
    pub fn new(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            jwks_url: None,
            subject_claim: "sub".to_string(),
            leeway_seconds: 30,
            cache_ttl: Duration::from_secs(300),
            token_cache_capacity: NonZeroUsize::new(DEFAULT_TOKEN_CACHE_CAP).unwrap(),
        }
    }

    pub fn subject_claim(mut self, claim: impl Into<String>) -> Self {
        self.subject_claim = claim.into();
        self
    }

    pub fn jwks_url(mut self, url: impl Into<String>) -> Self {
        self.jwks_url = Some(url.into());
        self
    }

    pub fn token_cache_capacity(mut self, cap: NonZeroUsize) -> Self {
        self.token_cache_capacity = cap;
        self
    }

    fn build(self, jwks_url: String) -> Arc<OidcVerifier> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client builds");
        Arc::new(OidcVerifier {
            issuer: self.issuer,
            audience: self.audience,
            jwks_url,
            subject_claim: self.subject_claim,
            leeway_seconds: self.leeway_seconds,
            cache_ttl: self.cache_ttl,
            http,
            cache: RwLock::new(JwksCache::default()),
            token_cache: Mutex::new(LruCache::new(self.token_cache_capacity)),
        })
    }
}

impl OidcVerifier {
    /// Construct a verifier by discovering the JWKS endpoint from the
    /// issuer's OIDC discovery document. Pre-warms the JWKS cache so
    /// the first verified request after startup doesn't pay the
    /// issuer round-trip latency.
    pub async fn discover(
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Result<Arc<Self>, VerificationError> {
        let builder = OidcVerifierBuilder::new(issuer, audience);
        Self::discover_with_builder(builder).await
    }

    /// Discover with a custom builder (e.g. for a custom subject-claim
    /// or LRU capacity).
    pub async fn discover_with_builder(
        builder: OidcVerifierBuilder,
    ) -> Result<Arc<Self>, VerificationError> {
        Self::discover_inner(builder).await
    }

    #[instrument(skip_all, fields(issuer = %builder.issuer, audience = %builder.audience, subject_claim = %builder.subject_claim))]
    async fn discover_inner(
        builder: OidcVerifierBuilder,
    ) -> Result<Arc<Self>, VerificationError> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            builder.issuer.trim_end_matches('/')
        );
        let probe = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest builds");
        let doc: DiscoveryDocument = probe
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
        let verifier = builder.build(doc.jwks_uri);
        // Pre-warm the JWKS cache — fail startup if we can't reach the
        // issuer at boot. Better to crash now than to 500 the first
        // production request.
        verifier.refresh_jwks().await?;
        info!(
            issuer = %verifier.issuer,
            jwks_url = %verifier.jwks_url,
            "OIDC verifier ready (JWKS pre-warmed)"
        );
        Ok(verifier)
    }

    /// Construct a verifier with an explicit JWKS URL — used in tests
    /// and in environments where the issuer does not expose an OIDC
    /// discovery document. Does NOT pre-warm; tests typically seed the
    /// cache directly.
    pub fn with_jwks_url(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        jwks_url: impl Into<String>,
    ) -> Arc<Self> {
        OidcVerifierBuilder::new(issuer, audience).build(jwks_url.into())
    }

    /// Verify a Bearer token's signature + standard claims. First
    /// checks the verified-token LRU cache (R-AUTH-4); on miss, runs
    /// the full signature + JWKS path and inserts into the cache.
    pub async fn verify(&self, token: &str) -> Result<Claims, VerificationError> {
        // 1. LRU cache lookup. A hit on a still-valid token returns
        //    immediately without touching the signature.
        let now_unix = OffsetDateTime::now_utc().unix_timestamp();
        {
            let mut cache = self.token_cache.lock().await;
            if let Some((claims, exp)) = cache.get(token) {
                if *exp > now_unix.saturating_add(self.leeway_seconds as i64) {
                    return Ok(claims.clone());
                }
                // Expired — evict eagerly so we re-verify and refill.
                cache.pop(token);
            }
        }

        // 2. Real verification: header → JWKS lookup → signature +
        //    claim validation.
        let claims = self.verify_uncached(token).await?;

        // 3. Insert into LRU. Cache TTL is the token's own `exp` — no
        //    additional clock; if the same token presents later, we
        //    re-verify naturally.
        let mut cache = self.token_cache.lock().await;
        cache.put(token.to_string(), (claims.clone(), claims.exp));
        Ok(claims)
    }

    /// Verification without the LRU shortcut — used internally and
    /// exposed for tests that need to exercise the full path.
    #[instrument(skip_all)]
    pub async fn verify_uncached(&self, token: &str) -> Result<Claims, VerificationError> {
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

        // Deserialise into a JSON Value so we can read any custom
        // claim, then extract the standard ones we need.
        let data = decode::<JsonValue>(token, &key, &validation)?;
        let raw = data.claims;

        let iss = raw
            .get("iss")
            .and_then(|v| v.as_str())
            .ok_or(VerificationError::MissingClaim("iss"))?
            .to_string();
        let exp = raw
            .get("exp")
            .and_then(|v| v.as_i64())
            .ok_or(VerificationError::MissingClaim("exp"))?;
        let sub = raw
            .get(&self.subject_claim)
            .and_then(|v| v.as_str())
            .ok_or_else(|| VerificationError::SubjectClaimAbsent {
                claim: self.subject_claim.clone(),
            })?
            .to_string();
        if sub.is_empty() {
            return Err(VerificationError::SubjectClaimAbsent {
                claim: self.subject_claim.clone(),
            });
        }

        Ok(Claims { sub, iss, exp, raw })
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

    use super::*;

    const TEST_ISSUER: &str = "https://issuer.test";
    const TEST_AUDIENCE: &str = "recor-test-aud";
    const TEST_KID: &str = "test-key-1";

    fn rs256_keypair() -> (EncodingKey, jsonwebtoken::jwk::Jwk) {
        const PEM: &str = include_str!("../tests/fixtures/test_rsa_pkcs8.pem");
        let encoding =
            EncodingKey::from_rsa_pem(PEM.as_bytes()).expect("test RSA PEM is valid");
        let jwk_json = serde_json::from_str::<serde_json::Value>(include_str!(
            "../tests/fixtures/test_rsa_jwk.json"
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
        make_verifier_with_subject_claim(jwks, "sub")
    }

    fn make_verifier_with_subject_claim(jwks: JwkSet, claim: &str) -> Arc<OidcVerifier> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        Arc::new(OidcVerifier {
            issuer: TEST_ISSUER.to_string(),
            audience: TEST_AUDIENCE.to_string(),
            jwks_url: "http://127.0.0.1:0/jwks-unreachable".to_string(),
            subject_claim: claim.to_string(),
            leeway_seconds: 30,
            cache_ttl: Duration::from_secs(60),
            http,
            cache: RwLock::new(JwksCache {
                jwks: Some(jwks),
                fetched_at: Some(Instant::now()),
            }),
            token_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_TOKEN_CACHE_CAP).unwrap(),
            )),
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
    async fn second_verification_hits_lru_cache() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 300,
            }),
            &signing,
        );
        // First call: full verification + cache insert.
        let first = v.verify(&token).await.unwrap();
        // Second call: cache hit.
        let second = v.verify(&token).await.unwrap();
        assert_eq!(first.sub, second.sub);
        // Cache should now contain the token.
        let cache = v.token_cache.lock().await;
        assert!(cache.contains(token.as_str()));
    }

    #[tokio::test]
    async fn cache_evicts_expired_entry_on_lookup() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 60,
            }),
            &signing,
        );
        // Populate the cache with a token whose exp is in the PAST so
        // the cache.get path treats it as expired and evicts.
        let stale = Claims {
            sub: "x".to_string(),
            iss: TEST_ISSUER.to_string(),
            exp: now() - 1_000,
            raw: serde_json::Value::Null,
        };
        v.token_cache
            .lock()
            .await
            .put(token.clone(), (stale, now() - 1_000));
        // verify() must NOT return the stale entry; it must re-verify
        // and replace it.
        let claims = v.verify(&token).await.unwrap();
        assert!(claims.exp > now()); // got a fresh entry from re-verification
    }

    #[tokio::test]
    async fn configurable_subject_claim_uses_custom_field() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier_with_subject_claim(
            JwkSet { keys: vec![jwk] },
            "preferred_username",
        );
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "internal-uuid-only",
                "preferred_username": "alice@recor.cm",
                "exp": now() + 300,
            }),
            &signing,
        );
        let claims = v.verify(&token).await.unwrap();
        assert_eq!(claims.sub, "alice@recor.cm");
    }

    #[tokio::test]
    async fn configurable_subject_claim_missing_field_rejects() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier_with_subject_claim(
            JwkSet { keys: vec![jwk] },
            "preferred_username",
        );
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "anything",
                // preferred_username deliberately omitted
                "exp": now() + 300,
            }),
            &signing,
        );
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, VerificationError::SubjectClaimAbsent { .. }));
    }

    #[tokio::test]
    async fn raw_claims_carry_arbitrary_payload() {
        let (signing, jwk) = rs256_keypair();
        let v = make_verifier(JwkSet { keys: vec![jwk] });
        let token = sign(
            json!({
                "iss": TEST_ISSUER,
                "aud": TEST_AUDIENCE,
                "sub": "x",
                "exp": now() + 300,
                "scope": "decl:write",
                "tenant_id": "recor-cm-prod",
            }),
            &signing,
        );
        let claims = v.verify(&token).await.unwrap();
        assert_eq!(claims.raw.get("scope").unwrap().as_str(), Some("decl:write"));
        assert_eq!(
            claims.raw.get("tenant_id").unwrap().as_str(),
            Some("recor-cm-prod"),
        );
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
                "exp": now() - 3600,
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

    // ─── TODO-020 — IAL/AAL ACR-claim parsing + enforcement ──────────

    #[test]
    fn assurance_level_orders_correctly() {
        assert!(AssuranceLevel::Ial1 < AssuranceLevel::Ial2);
        assert!(AssuranceLevel::Ial2 < AssuranceLevel::Ial3);
    }

    #[test]
    fn parses_nist_uri_acr_values() {
        assert_eq!(
            AssuranceLevel::from_acr_claim("http://idmanagement.gov/ns/assurance/ial/3"),
            AssuranceLevel::Ial3
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim("https://idmanagement.gov/ns/assurance/ial/2"),
            AssuranceLevel::Ial2
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim("http://idmanagement.gov/ns/assurance/ial/1"),
            AssuranceLevel::Ial1
        );
    }

    #[test]
    fn parses_numeric_acr_values() {
        assert_eq!(
            AssuranceLevel::from_acr_claim("1"),
            AssuranceLevel::Ial1
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim("2"),
            AssuranceLevel::Ial2
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim("3"),
            AssuranceLevel::Ial3
        );
        // ISO/IEC 29115 sometimes uses "0" for self-asserted; map to
        // Ial1 (the floor).
        assert_eq!(
            AssuranceLevel::from_acr_claim("0"),
            AssuranceLevel::Ial1
        );
    }

    #[test]
    fn parses_short_acr_values() {
        assert_eq!(
            AssuranceLevel::from_acr_claim("IAL3"),
            AssuranceLevel::Ial3
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim("ial2"),
            AssuranceLevel::Ial2
        );
    }

    #[test]
    fn unknown_acr_value_floors_to_ial1() {
        // The fail-closed floor is Ial1 — the caller's
        // `enforce_assurance(Ial2)` will then refuse. This is
        // intentional: an unrecognised acr value MUST NOT be silently
        // upgraded.
        assert_eq!(
            AssuranceLevel::from_acr_claim("urn:something:weird"),
            AssuranceLevel::Ial1
        );
        assert_eq!(
            AssuranceLevel::from_acr_claim(""),
            AssuranceLevel::Ial1
        );
    }

    #[test]
    fn claims_default_to_ial1_when_acr_absent() {
        let claims = Claims {
            sub: "s".into(),
            iss: "i".into(),
            exp: 0,
            raw: json!({}),
        };
        assert_eq!(claims.assurance_level(), AssuranceLevel::Ial1);
    }

    #[test]
    fn enforce_assurance_passes_at_or_above_threshold() {
        let claims = Claims {
            sub: "s".into(),
            iss: "i".into(),
            exp: 0,
            raw: json!({"acr": "http://idmanagement.gov/ns/assurance/ial/3"}),
        };
        assert!(claims.enforce_assurance(AssuranceLevel::Ial1).is_ok());
        assert!(claims.enforce_assurance(AssuranceLevel::Ial2).is_ok());
        assert!(claims.enforce_assurance(AssuranceLevel::Ial3).is_ok());
    }

    #[test]
    fn enforce_assurance_refuses_below_threshold() {
        let claims = Claims {
            sub: "s".into(),
            iss: "i".into(),
            exp: 0,
            raw: json!({"acr": "1"}),
        };
        let err = claims
            .enforce_assurance(AssuranceLevel::Ial2)
            .unwrap_err();
        assert!(matches!(
            err,
            VerificationError::InsufficientAssurance { ref presented, required: AssuranceLevel::Ial2 }
                if presented == "1"
        ));
    }

    #[test]
    fn enforce_assurance_refuses_absent_claim_when_strict_minimum_demanded() {
        let claims = Claims {
            sub: "s".into(),
            iss: "i".into(),
            exp: 0,
            raw: json!({}),
        };
        let err = claims
            .enforce_assurance(AssuranceLevel::Ial2)
            .unwrap_err();
        assert!(matches!(
            err,
            VerificationError::InsufficientAssurance { ref presented, .. }
                if presented == "<absent>"
        ));
    }
}
