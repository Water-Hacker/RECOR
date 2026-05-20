//! Authentication. Two paths:
//!
//!   - Production: OIDC Bearer-token verification against the
//!     configured issuer's JWKS. Real signature + iss + aud + exp +
//!     nbf checking via `crate::api::oidc::OidcVerifier`. The verifier
//!     is constructed at startup; the middleware shares an `Arc<_>`.
//!   - Dev: an HS256-equivalent static key shortcut is NOT used; we
//!     accept a special `X-Recor-Dev-Principal` header that asserts
//!     the principal name. This is gated by `Config::is_dev()` and
//!     refused otherwise.
//!
//! D14 (fail-closed): bearer-token requests with no verifier configured
//! are rejected with 401, not silently allowed through. The config
//! layer refuses to start outside dev when `OIDC_ISSUER_URL` is empty,
//! so a production deployment cannot land in the "no verifier" state.
//!
//! D17: every request that reaches the protected handler MUST have a
//! verified principal in the request extensions.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::warn;

use crate::api::oidc::{AssuranceLevel, OidcVerifier, VerificationError};
use crate::error::ServiceError;
use crate::metrics::Metrics;

#[derive(Debug, Clone)]
pub struct Principal {
    pub subject: String,
    pub source: PrincipalSource,
    /// TODO-020 — Identity Assurance Level resolved from the OIDC
    /// `acr` claim (production) or the `X-Recor-Dev-Acr` dev header
    /// (default `IAL3`, so existing dev tests do not regress). The
    /// fail-closed floor when nothing is presented is `IAL1`.
    pub assurance_level: AssuranceLevel,
    /// TODO-006 — Sovim-tiered principal class. Resolved from the
    /// verified OIDC `scope` claim (production) or the
    /// `X-Recor-Dev-Class` header (dev). The default — when no
    /// recognised scope is present and the principal is not on any
    /// admin allowlist — is [`PrincipalClass::Declarant`]; admin
    /// allowlist membership upgrades to [`PrincipalClass::Admin`].
    /// The class is the platform-wide policy gate for FIU / public-
    /// feedback / obliged-entity surfaces.
    pub class: PrincipalClass,
}

/// TODO-006 / TODO-008 / TODO-009 — Sovim-tiered principal class.
///
/// Mirrors the post-Sovim balancing test (CJEU C-37/20 + C-601/20)
/// and FATF R.24 c.24.9 (FIU access). The class is **NOT**
/// authoritative on its own — the per-endpoint `require_class` check,
/// plus the admin-allowlist mechanism, are what actually gate access.
/// The class is the bookkeeping that lets the gates be coherent
/// across handlers.
///
/// - **Admin** — operator on the per-service `ADMIN_PRINCIPALS`
///   allowlist; full payload, every endpoint.
/// - **FiuAnif** — ANIF (Cameroon FIU) or, via R.40 MLAT, a foreign
///   FIU routed through Egmont. Carries the
///   `recor:fiu-anif` scope AND must additionally pass the mTLS peer-
///   ID allowlist + IP allowlist gates (defence in depth — handled
///   outside this class enum). FATF c.24.9 requires real-time access
///   for FIUs; every disclosure is event-sourced in
///   `fiu_disclosure_log` (TODO-008).
/// - **ObligedEntity** — regulated counter-party (FI under
///   COBAC/BEAC; DNFBP under the local supervisory regime) carrying
///   a verified `recor:obliged-entity` scope. Sees the reduced
///   payload (post-Sovim: no national-ID, no residential address,
///   no biometric hash) and is logged for every BO disclosure.
/// - **PublicFeedback** — anonymous or pseudonymous member of the
///   public reporting a registry inaccuracy under
///   `recor:public-feedback`. May NOT read declarations; the only
///   endpoint they may call is `POST /v1/public-feedback`
///   (TODO-009). CAPTCHA + IP rate-limit gated.
/// - **Declarant** — natural person submitting their own declarations
///   or representing an entity they have a verified affiliation
///   with. Default class for any authenticated principal that is
///   not on the admin allowlist and does not present a
///   recognised supervisory scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalClass {
    Admin,
    FiuAnif,
    ObligedEntity,
    PublicFeedback,
    Declarant,
}

impl PrincipalClass {
    /// Parse the OIDC `scope` claim (space-delimited) into the most
    /// privileged class the claim's scopes resolve to. Note that
    /// **admin is NEVER expressible via the scope claim alone** — the
    /// admin allowlist membership is the authoritative source for
    /// `Admin`. The ordering implements the strict ladder
    /// FiuAnif > ObligedEntity > PublicFeedback > Declarant.
    pub fn from_scope_claim(scope: &str) -> Self {
        let mut found = PrincipalClass::Declarant;
        for s in scope.split_whitespace() {
            if s == "recor:fiu-anif" || s.starts_with("recor:fiu-anif:") {
                return PrincipalClass::FiuAnif;
            }
            if s == "recor:obliged-entity" || s.starts_with("recor:obliged-entity:") {
                found = PrincipalClass::ObligedEntity;
            }
            if (s == "recor:public-feedback"
                || s.starts_with("recor:public-feedback:"))
                && found == PrincipalClass::Declarant
            {
                // PublicFeedback ranks below ObligedEntity — only adopt
                // it if we have not already promoted to ObligedEntity.
                found = PrincipalClass::PublicFeedback;
            }
        }
        found
    }

    /// Parse the dev-mode `X-Recor-Dev-Class` header. Case-insensitive.
    /// Unknown / missing → [`PrincipalClass::Declarant`].
    pub fn from_dev_header(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => PrincipalClass::Admin,
            "fiu-anif" | "fiu_anif" | "fiu" => PrincipalClass::FiuAnif,
            "obliged-entity" | "obliged_entity" => PrincipalClass::ObligedEntity,
            "public-feedback" | "public_feedback" => PrincipalClass::PublicFeedback,
            _ => PrincipalClass::Declarant,
        }
    }
}

impl Principal {
    /// Refuse this principal if their resolved assurance level is below
    /// `min`. Returns the 403-bearing service error
    /// [`ServiceError::AuthorizationDenied("insufficient_assurance")`]
    /// so the caller knows step-up authentication is required.
    pub fn require_assurance(&self, min: AssuranceLevel) -> Result<(), ServiceError> {
        if self.assurance_level >= min {
            Ok(())
        } else {
            tracing::warn!(
                subject = %self.subject,
                presented = ?self.assurance_level,
                required = ?min,
                "TODO-020: assurance-level gate refused submission"
            );
            Err(ServiceError::AuthorizationDenied("insufficient_assurance"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalSource {
    DevHeader,
    Bearer,
}

/// Shared state for the auth middleware. `None` for `oidc` means no
/// verifier was configured at startup — bearer tokens are then rejected.
/// Dev-header path still works if `is_dev == true`.
#[derive(Clone)]
pub struct AuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<OidcVerifier>>,
    /// OBS-1: shared Prometheus registry so the middleware can record
    /// per-verify outcomes (`recor_oidc_verify_total{result}`). The
    /// label `result` is a 3-value bounded enum (D18).
    pub metrics: Arc<Metrics>,
}

/// Axum middleware that resolves the request principal and inserts it
/// into request extensions. Handlers extract it via the `RequirePrincipal`
/// extractor.
pub async fn auth_middleware(
    State(state): State<AuthConfig>,
    mut req: Request,
    next: Next,
) -> Result<Response, ServiceError> {
    let principal = resolve_principal(req.headers(), &state).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

async fn resolve_principal(
    headers: &HeaderMap,
    state: &AuthConfig,
) -> Result<Principal, ServiceError> {
    // Dev-only shortcut: X-Recor-Dev-Principal header.
    if state.is_dev {
        if let Some(value) = headers.get("x-recor-dev-principal") {
            let subject = value
                .to_str()
                .map_err(|_| ServiceError::BadRequest("malformed dev principal header".into()))?
                .trim()
                .to_string();
            if subject.is_empty() {
                return Err(ServiceError::BadRequest(
                    "empty dev principal header".into(),
                ));
            }
            // TODO-020 — dev-header path: default to IAL3 so existing
            // dev tests work unchanged. `X-Recor-Dev-Acr` lets tests
            // exercise step-up gates explicitly. Dev mode is structurally
            // refused outside `ENVIRONMENT=dev` (FIND-003), so this
            // backdoor cannot land in production.
            let assurance_level = headers
                .get("x-recor-dev-acr")
                .and_then(|v| v.to_str().ok())
                .map(AssuranceLevel::from_acr_claim)
                .unwrap_or(AssuranceLevel::Ial3);
            // TODO-006 — dev path: explicit class header, default Declarant.
            // Production never reaches this branch (FIND-003 refuses
            // `ENVIRONMENT=dev` outside dev).
            let class = headers
                .get("x-recor-dev-class")
                .and_then(|v| v.to_str().ok())
                .map(PrincipalClass::from_dev_header)
                .unwrap_or(PrincipalClass::Declarant);
            return Ok(Principal {
                subject,
                source: PrincipalSource::DevHeader,
                assurance_level,
                class,
            });
        }
    }

    // Bearer token path.
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        return Err(ServiceError::AuthenticationRequired);
    };

    let Some(verifier) = state.oidc.as_ref() else {
        // Defensive: should not be reachable in production because
        // Config refuses to start when OIDC_ISSUER_URL is unset and
        // environment != "dev". Log loudly if we see it anyway.
        warn!("bearer token received but no OIDC verifier configured");
        return Err(ServiceError::AuthenticationRequired);
    };

    let claims = verifier.verify(token).await.map_err(|e| {
        warn!(error = %e, "bearer token failed verification");
        // OBS-1: bounded-cardinality outcome label. `unavailable` is
        // an infrastructure fault (JWKS / discovery 5xx) — distinct
        // from a client-side `invalid` so on-call can tell apart
        // "OIDC backend down" from "bad tokens flooding in".
        let label = match &e {
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => "unavailable",
            _ => "invalid",
        };
        state
            .metrics
            .oidc_verify_total
            .with_label_values(&[label])
            .inc();
        match e {
            VerificationError::TokenInvalid(_)
            | VerificationError::MalformedHeader
            | VerificationError::MissingKid
            | VerificationError::UnknownKid(_)
            | VerificationError::UnsupportedAlgorithm(_)
            | VerificationError::NoUsableKey
            | VerificationError::MissingClaim(_)
            | VerificationError::InsufficientAssurance { .. }
            | VerificationError::SubjectClaimAbsent { .. } => {
                ServiceError::AuthenticationRequired
            }
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => ServiceError::Internal,
        }
    })?;

    if claims.sub.trim().is_empty() {
        state
            .metrics
            .oidc_verify_total
            .with_label_values(&["invalid"])
            .inc();
        return Err(ServiceError::AuthenticationRequired);
    }
    state
        .metrics
        .oidc_verify_total
        .with_label_values(&["success"])
        .inc();
    // TODO-020 — resolve the IAL/AAL the IdP advertised on this token.
    // The fail-closed floor when `acr` is absent is `Ial1`; each
    // endpoint's `Principal::require_assurance` call then refuses the
    // request if the policy demands more.
    let assurance_level = claims.assurance_level();
    // TODO-006 — resolve the Sovim principal class. The admin
    // allowlist applies at the handler layer (it depends on the
    // service's `ADMIN_PRINCIPALS` config which isn't visible here);
    // this pass picks between Declarant and ObligedEntity.
    let class = claims
        .raw
        .get("scope")
        .and_then(|v| v.as_str())
        .map(PrincipalClass::from_scope_claim)
        .unwrap_or(PrincipalClass::Declarant);
    Ok(Principal {
        subject: claims.sub,
        source: PrincipalSource::Bearer,
        assurance_level,
        class,
    })
}

// Suppress unused warnings during partial build.
#[allow(dead_code)]
fn _force_imports(_b: Body, _s: StatusCode) {}

#[cfg(test)]
mod assurance_tests {
    use super::*;

    fn principal_at(level: AssuranceLevel) -> Principal {
        Principal {
            subject: "spiffe://recor.cm/test".into(),
            source: PrincipalSource::DevHeader,
            assurance_level: level,
            class: PrincipalClass::Declarant,
        }
    }

    #[test]
    fn ial2_principal_passes_ial2_gate() {
        principal_at(AssuranceLevel::Ial2)
            .require_assurance(AssuranceLevel::Ial2)
            .expect("Ial2 satisfies Ial2 minimum");
    }

    #[test]
    fn ial3_principal_passes_ial2_gate() {
        principal_at(AssuranceLevel::Ial3)
            .require_assurance(AssuranceLevel::Ial2)
            .expect("Ial3 satisfies Ial2 minimum");
    }

    #[test]
    fn ial1_principal_refused_ial2_gate() {
        let err = principal_at(AssuranceLevel::Ial1)
            .require_assurance(AssuranceLevel::Ial2)
            .unwrap_err();
        match err {
            ServiceError::AuthorizationDenied(reason) => {
                assert_eq!(reason, "insufficient_assurance")
            }
            other => panic!("expected AuthorizationDenied, got {other:?}"),
        }
    }

    #[test]
    fn ial2_principal_refused_ial3_gate() {
        let err = principal_at(AssuranceLevel::Ial2)
            .require_assurance(AssuranceLevel::Ial3)
            .unwrap_err();
        assert!(matches!(
            err,
            ServiceError::AuthorizationDenied("insufficient_assurance")
        ));
    }

    #[test]
    fn ial3_principal_passes_ial3_gate() {
        principal_at(AssuranceLevel::Ial3)
            .require_assurance(AssuranceLevel::Ial3)
            .expect("Ial3 satisfies Ial3 minimum");
    }
}

#[cfg(test)]
mod class_tests {
    use super::*;

    #[test]
    fn obliged_entity_scope_resolves_to_obliged_entity() {
        assert_eq!(
            PrincipalClass::from_scope_claim("openid recor:obliged-entity"),
            PrincipalClass::ObligedEntity
        );
    }

    #[test]
    fn obliged_entity_subscope_also_resolves() {
        // Sub-scopes (e.g. `recor:obliged-entity:cdd`) are part of the
        // TODO-006 acceptance criterion #1 (per-supervision-class
        // scopes).
        assert_eq!(
            PrincipalClass::from_scope_claim("recor:obliged-entity:cdd"),
            PrincipalClass::ObligedEntity
        );
    }

    #[test]
    fn unknown_scope_defaults_to_declarant() {
        assert_eq!(
            PrincipalClass::from_scope_claim("openid profile"),
            PrincipalClass::Declarant
        );
        assert_eq!(
            PrincipalClass::from_scope_claim(""),
            PrincipalClass::Declarant
        );
    }

    #[test]
    fn dev_header_admin_resolves() {
        assert_eq!(
            PrincipalClass::from_dev_header("admin"),
            PrincipalClass::Admin
        );
        assert_eq!(
            PrincipalClass::from_dev_header("ADMIN"),
            PrincipalClass::Admin
        );
    }

    #[test]
    fn dev_header_obliged_entity_both_spellings() {
        assert_eq!(
            PrincipalClass::from_dev_header("obliged-entity"),
            PrincipalClass::ObligedEntity
        );
        assert_eq!(
            PrincipalClass::from_dev_header("obliged_entity"),
            PrincipalClass::ObligedEntity
        );
    }

    #[test]
    fn admin_is_not_scope_expressible() {
        // The scope claim alone MUST NOT upgrade a caller to Admin —
        // admin is gated on the per-service allowlist (REQ-d17-007).
        // Any scope containing `recor:admin` resolves to Declarant.
        // (Note that audit-verifier's separate AuthorizationTier
        // enum DOES recognise `recor:admin` at its read surface; the
        // declaration-service's *write* surface intentionally does
        // not.)
        assert_eq!(
            PrincipalClass::from_scope_claim("recor:admin"),
            PrincipalClass::Declarant
        );
    }
}

#[cfg(test)]
mod dto_redaction_tests {
    use crate::api::dto::GetDeclarationResponse;
    use crate::application::DeclarationProjection;
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::value_object::InterestKind;
    use crate::domain::{
        BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, DeclarationState,
        EntityId, OwnershipBasisPoints, PersonId,
    };
    use time::macros::date;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn projection() -> DeclarationProjection {
        DeclarationProjection {
            declaration_id: DeclarationId(Uuid::now_v7()),
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: "spiffe://recor.cm/declarant-A".into(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: vec![BeneficialOwnerClaim {
                person_id: PersonId(Uuid::now_v7()),
                ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(10_000)
                    .unwrap(),
                interest_kind: InterestKind::Equity,
                cascade_tier: None,
                control_basis: None,
                cascade_tier_b_ruled_out_evidence: Some(
                    "internal note: see exhibit-B email chain".into(),
                ),
                is_nominee: Some(true),
                nominator_person_id: Some(PersonId(Uuid::now_v7())),
            }],
            attestation: CryptographicAttestation {
                signed_by: "spiffe://recor.cm/declarant-A".into(),
                signature_algorithm: SignatureAlgorithm::Ed25519,
                signature_hex: "00".repeat(64),
                public_key_hex: "11".repeat(32),
                nonce_hex: "22".repeat(16),
            },
            state: DeclarationState::Submitted,
            version: 1,
            submitted_at: OffsetDateTime::now_utc(),
            receipt_hash_hex: "ab".repeat(32),
            correlation_id: Uuid::now_v7(),
            verification_state: "not_verified".to_string(),
            verification_lane: None,
            verification_case_id: Some(Uuid::now_v7()),
            verified_at: None,
            supersedes_declaration_id: None,
            superseded_by_declaration_id: None,
            superseded_at: None,
            amended_at: None,
            metadata_notes: None,
            corrected_at: None,
        }
    }

    #[test]
    fn obliged_entity_redactor_strips_sensitive_fields() {
        let response: GetDeclarationResponse = projection().into();
        let original_bo_id = response.beneficial_owners[0].person_id.0;
        let redacted = response.clone().redact_for_obliged_entity();

        // declarant_principal removed
        assert_eq!(redacted.declarant_principal, "");
        // correlation_id zeroed
        assert_eq!(redacted.correlation_id, Uuid::nil());
        // verification_case_id removed
        assert!(redacted.verification_case_id.is_none());

        // BO graph kept — that's what the obliged entity needs.
        assert_eq!(
            redacted.beneficial_owners[0].person_id.0,
            original_bo_id,
            "BO person_id preserved"
        );
        // BO-level Sovim-sensitive fields stripped.
        assert!(
            redacted.beneficial_owners[0]
                .cascade_tier_b_ruled_out_evidence
                .is_none(),
            "free-text evidence stripped"
        );
        assert!(
            redacted.beneficial_owners[0].nominator_person_id.is_none(),
            "nominator_person_id stripped"
        );
        // Admin/owner-visible field preserved.
        assert_eq!(
            redacted.beneficial_owners[0].ownership_basis_points,
            response.beneficial_owners[0].ownership_basis_points,
            "ownership_basis_points preserved (CDD needs this)"
        );
    }
}
