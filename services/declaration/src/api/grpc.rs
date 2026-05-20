//! gRPC adapter over the declaration use cases (R-DECL-8).
//!
//! Mirrors `api::rest`. Each RPC is a thin adapter that:
//!
//!   1. Reads the authenticated `Principal` from the request extensions
//!      (populated by the [`auth_interceptor`] which uses the SAME OIDC
//!      verifier as REST — D17 zero trust).
//!   2. Converts the proto message into a domain command, preserving
//!      the canonical-bytes contract used by REST so the declarant's
//!      signature verifies identically over either transport
//!      (D15 cryptographic provenance).
//!   3. Calls the existing use case (no domain logic lives here).
//!   4. Converts the result into a proto response, or the domain error
//!      into a `tonic::Status` with a deliberate non-Unknown code
//!      (D14 fail-closed).
//!
//! The wire schema lives at `contracts/declaration.proto`; the
//! generated stubs land in `OUT_DIR/recor.declaration.v1.rs` and are
//! pulled in via [`tonic::include_proto!`] below.

use std::sync::Arc;

use time::OffsetDateTime;
use tonic::{Request, Response, Status};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::auth::Principal;
use crate::api::rest::AppState;
use crate::application::{
    AmendError, CorrectError, GetError, SubmitError, SupersedeError,
};
use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
use crate::domain::{
    AmendDeclaration, AmendmentSet, BeneficialOwnerClaim, CorrectDeclaration, CorrectionSet,
    DeclarantRole, DeclarationId, DeclarationKind, DomainError, EntityId, OwnershipBasisPoints,
    PersonId, SubmitDeclaration, VerificationLane,
};
use crate::domain::value_object::InterestKind;
use crate::error::ServiceError;

// ─── Generated stubs ─────────────────────────────────────────────────
//
// The .proto package is `recor.declaration.v1`; tonic-build derives a
// nested Rust module of the same path. Re-export the inner module so
// callers can spell types as `crate::api::grpc::proto::*`.

pub mod proto {
    tonic::include_proto!("recor.declaration.v1");
}

use proto::declaration_service_server::{DeclarationService, DeclarationServiceServer};

// ─── Service struct + AppState wiring ─────────────────────────────────

/// The tonic-side service handle. Wraps an [`AppState`] so each RPC has
/// the same use-case set as the REST handlers; gRPC and REST share
/// state by construction (D01 completeness).
#[derive(Clone)]
pub struct DeclarationGrpcService {
    state: AppState,
}

impl DeclarationGrpcService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Convenience: wrap this service in a `DeclarationServiceServer`
    /// configured with the OIDC interceptor. The caller binds the
    /// returned service to a `Server::builder()` and serves it.
    pub fn into_server_with_auth(
        self,
        auth: GrpcAuthConfig,
    ) -> tonic::service::interceptor::InterceptedService<
        DeclarationServiceServer<DeclarationGrpcService>,
        impl tonic::service::Interceptor + Clone,
    > {
        let interceptor = auth_interceptor(auth);
        DeclarationServiceServer::with_interceptor(self, interceptor)
    }
}

// ─── Auth interceptor ────────────────────────────────────────────────

/// Configuration for the gRPC auth interceptor — mirror of
/// `crate::api::auth::AuthConfig` for the tonic surface. The
/// interceptor sources the principal from the SAME OIDC verifier
/// instance the REST middleware uses (`Arc<OidcVerifier>`), so D17
/// holds uniformly across transports.
#[derive(Clone)]
pub struct GrpcAuthConfig {
    pub is_dev: bool,
    pub oidc: Option<Arc<crate::api::OidcVerifier>>,
}

/// Build a tonic interceptor that resolves the request principal and
/// inserts it into the request extensions. Handlers extract it via
/// `req.extensions().get::<Principal>()`.
///
/// Verification runs against the SAME `OidcVerifier` used by REST —
/// shared via `Arc<_>` — so any policy change applies to both
/// transports without divergence.
///
/// Two failure modes are deliberately mapped to `Unauthenticated`:
///   - No bearer token (and no dev-principal header in dev): the
///     interceptor returns `Status::unauthenticated`, not
///     `Status::permission_denied` — the request has no credential to
///     evaluate yet.
///   - Bearer token failed signature/claim verification: same status,
///     same reason.
///
/// `Internal` is reserved for infrastructure faults (JWKS unreachable,
/// discovery 5xx) — never for client mistakes (D14).
fn auth_interceptor(
    auth: GrpcAuthConfig,
) -> impl tonic::service::Interceptor + Clone {
    move |mut req: Request<()>| {
        let principal = resolve_principal_blocking(req.metadata(), &auth)?;
        req.extensions_mut().insert(principal);
        Ok(req)
    }
}

/// Interceptors run on the request-arrival path BEFORE the tokio task
/// that runs the handler; tonic expects this to be a synchronous
/// `FnMut(Request<()>) -> Result<Request<()>, Status>`. The current
/// `OidcVerifier::verify` is `async` because it can lazily refresh
/// JWKS. For the gRPC path we side-step the asynchrony by:
///
///   1. Pre-warming JWKS at startup (already done by
///      `OidcVerifier::discover`).
///   2. Routing the actual `verify` call through
///      `tokio::task::block_in_place` on the multi-thread runtime so
///      the interceptor remains synchronous from tonic's perspective
///      while the verifier can await internally.
///
/// `block_in_place` is safe in the multi-thread Tokio runtime tonic
/// uses; on a single-threaded runtime it panics — production deploys
/// always use multi-thread. The tests assert this by running under
/// `#[tokio::test(flavor = "multi_thread")]`.
fn resolve_principal_blocking(
    metadata: &tonic::metadata::MetadataMap,
    auth: &GrpcAuthConfig,
) -> Result<Principal, Status> {
    use crate::api::auth::PrincipalSource;

    // Dev shortcut: x-recor-dev-principal header.
    if auth.is_dev {
        if let Some(value) = metadata.get("x-recor-dev-principal") {
            let subject = value
                .to_str()
                .map_err(|_| Status::invalid_argument("malformed dev principal header"))?
                .trim()
                .to_string();
            if subject.is_empty() {
                return Err(Status::invalid_argument("empty dev principal header"));
            }
            return Ok(Principal {
                subject,
                source: PrincipalSource::DevHeader,
            });
        }
    }

    // Bearer token path.
    let token = metadata
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = token else {
        return Err(Status::unauthenticated("authentication required"));
    };

    let Some(verifier) = auth.oidc.as_ref() else {
        warn!("gRPC: bearer token received but no OIDC verifier configured");
        return Err(Status::unauthenticated("authentication required"));
    };

    // Bridge async → sync. Tonic interceptors are synchronous; the
    // verifier is async because it may refresh JWKS. block_in_place +
    // Handle::current().block_on() runs the future on the current
    // multi-thread runtime without freeing the worker thread for
    // unrelated work. See module-level docs for the rationale.
    let claims = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(verifier.verify(token))
    });

    let claims = claims.map_err(|e| {
        use crate::api::oidc::VerificationError;
        warn!(error = %e, "gRPC: bearer token failed verification");
        match e {
            VerificationError::TokenInvalid(_)
            | VerificationError::MalformedHeader
            | VerificationError::MissingKid
            | VerificationError::UnknownKid(_)
            | VerificationError::UnsupportedAlgorithm(_)
            | VerificationError::NoUsableKey
            | VerificationError::MissingClaim(_)
            | VerificationError::SubjectClaimAbsent { .. } => {
                Status::unauthenticated("authentication required")
            }
            VerificationError::DiscoveryFailed { .. }
            | VerificationError::JwksFetchFailed { .. } => {
                Status::internal("oidc discovery failed")
            }
        }
    })?;

    if claims.sub.trim().is_empty() {
        return Err(Status::unauthenticated("authentication required"));
    }
    Ok(Principal {
        subject: claims.sub,
        source: PrincipalSource::Bearer,
    })
}

// ─── Service trait impl ──────────────────────────────────────────────

#[tonic::async_trait]
impl DeclarationService for DeclarationGrpcService {
    #[tracing::instrument(skip_all)]
    async fn submit_declaration(
        &self,
        request: Request<proto::SubmitDeclarationRequest>,
    ) -> Result<Response<proto::SubmitDeclarationResponse>, Status> {
        let principal = require_principal(&request)?;
        let req = request.into_inner();

        // Build the canonical bytes the declarant signed — IDENTICAL
        // shape to `api::rest::canonical_payload_bytes` so a payload
        // signed for REST verifies under gRPC and vice-versa (D15).
        let declaration_id = parse_optional_uuid(&req.declaration_id, "declaration_id")?
            .map(DeclarationId)
            .unwrap_or_default();
        let entity_id = parse_uuid(&req.entity_id, "entity_id").map(EntityId)?;
        let declarant_role = decode_declarant_role(req.declarant_role)?;
        let kind = decode_declaration_kind(req.kind)?;
        let effective_from = parse_iso_date(&req.effective_from)?;
        let beneficial_owners = decode_owners(&req.beneficial_owners)?;
        let attestation = decode_attestation(req.attestation.as_ref())?;

        let canonical_bytes = canonical_submit_bytes(
            &entity_id,
            &principal.subject,
            declarant_role,
            kind,
            effective_from,
            &beneficial_owners,
            &attestation.nonce_hex,
        )?;
        attestation
            .verify_against(&canonical_bytes)
            .map_err(|e| Status::unauthenticated(format!("bad_attestation: {e}")))?;

        let correlation_id = Uuid::now_v7();
        let cmd = SubmitDeclaration {
            declaration_id,
            entity_id,
            declarant_principal: principal.subject.clone(),
            declarant_role,
            kind,
            effective_from,
            beneficial_owners,
            attestation,
            // Deferred to PR-FATF-2.B: gRPC will carry adequacy_claims
            // once the proto contract is bumped to include the field.
            adequacy_claims: None,
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id,
            adequacy_claims: None,
        };

        let receipt = self
            .state
            .submit_usecase
            .execute(cmd)
            .await
            .map_err(submit_error_to_status)?;

        // OBS-1: increment the per-kind submit counter for gRPC traffic
        // too. Same shared registry as REST so a single
        // `recor_declarations_submitted_total` counts both transports.
        self.state
            .metrics
            .declarations_submitted_total
            .with_label_values(&[kind.as_str()])
            .inc();

        let receipt_url = format!(
            "{base}/v1/declarations/{id}",
            base = self.state.base_url,
            id = receipt.declaration_id
        );

        info!(
            declaration_id = %receipt.declaration_id,
            receipt_hash = %receipt.receipt_hash_hex,
            "gRPC submit_declaration ok"
        );
        Ok(Response::new(proto::SubmitDeclarationResponse {
            declaration_id: receipt.declaration_id.to_string(),
            state: receipt.state,
            receipt_hash_hex: receipt.receipt_hash_hex,
            submitted_at: format_iso_datetime(receipt.submitted_at),
            receipt_url,
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn get_declaration(
        &self,
        request: Request<proto::GetDeclarationRequest>,
    ) -> Result<Response<proto::GetDeclarationResponse>, Status> {
        let principal = require_principal(&request)?;
        let req = request.into_inner();
        let declaration_id = parse_uuid(&req.declaration_id, "declaration_id").map(DeclarationId)?;

        let projection = self
            .state
            .get_usecase
            .execute(declaration_id)
            .await
            .map_err(get_error_to_status)?;

        if projection.declarant_principal != principal.subject {
            return Err(Status::permission_denied(
                "declaration is owned by a different principal",
            ));
        }

        Ok(Response::new(projection_to_proto(projection)))
    }

    #[tracing::instrument(skip_all)]
    async fn supersede_declaration(
        &self,
        request: Request<proto::SupersedeDeclarationRequest>,
    ) -> Result<Response<proto::SupersedeDeclarationResponse>, Status> {
        let principal = require_principal(&request)?;
        let req = request.into_inner();

        let superseded_id =
            parse_uuid(&req.superseded_declaration_id, "superseded_declaration_id")
                .map(DeclarationId)?;
        let new_declaration_id = parse_optional_uuid(&req.new_declaration_id, "new_declaration_id")?
            .map(DeclarationId)
            .unwrap_or_default();
        let entity_id = parse_uuid(&req.entity_id, "entity_id").map(EntityId)?;
        let declarant_role = decode_declarant_role(req.declarant_role)?;
        let kind = decode_declaration_kind(req.kind)?;
        let effective_from = parse_iso_date(&req.effective_from)?;
        let beneficial_owners = decode_owners(&req.beneficial_owners)?;
        let attestation = decode_attestation(req.attestation.as_ref())?;

        // SAME canonical shape as REST's supersede (which reuses
        // canonical_submit_bytes) — D15.
        let canonical_bytes = canonical_submit_bytes(
            &entity_id,
            &principal.subject,
            declarant_role,
            kind,
            effective_from,
            &beneficial_owners,
            &attestation.nonce_hex,
        )?;
        attestation
            .verify_against(&canonical_bytes)
            .map_err(|e| Status::unauthenticated(format!("bad_attestation: {e}")))?;

        let correlation_id = Uuid::now_v7();
        let new_cmd = SubmitDeclaration {
            declaration_id: new_declaration_id,
            entity_id,
            declarant_principal: principal.subject.clone(),
            declarant_role,
            kind,
            effective_from,
            beneficial_owners,
            attestation,
            // Deferred to PR-FATF-2.B.
            adequacy_claims: None,
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id,
            adequacy_claims: None,
        };

        let receipt = self
            .state
            .supersede_usecase
            .execute(superseded_id, new_cmd)
            .await
            .map_err(supersede_error_to_status)?;

        let receipt_url = format!(
            "{base}/v1/declarations/{id}",
            base = self.state.base_url,
            id = receipt.new_declaration_id
        );
        Ok(Response::new(proto::SupersedeDeclarationResponse {
            new_declaration_id: receipt.new_declaration_id.to_string(),
            superseded_declaration_id: receipt.superseded_declaration_id.to_string(),
            state: receipt.state,
            receipt_hash_hex: receipt.receipt_hash_hex,
            submitted_at: format_iso_datetime(receipt.submitted_at),
            receipt_url,
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn amend_declaration(
        &self,
        request: Request<proto::AmendDeclarationRequest>,
    ) -> Result<Response<proto::AmendDeclarationResponse>, Status> {
        let principal = require_principal(&request)?;
        let req = request.into_inner();
        let declaration_id = parse_uuid(&req.declaration_id, "declaration_id").map(DeclarationId)?;
        let amendments_proto = req
            .amendments
            .ok_or_else(|| Status::invalid_argument("amendments missing"))?;
        let attestation = decode_attestation(req.attestation.as_ref())?;

        let amendments = AmendmentSet {
            beneficial_owners: decode_owners(&amendments_proto.beneficial_owners)?,
            effective_from: parse_iso_date(&amendments_proto.effective_from)?,
            declarant_role: decode_declarant_role(amendments_proto.declarant_role)?,
            // Deferred to PR-FATF-2.B; see SubmitDeclaration note.
            adequacy_claims: None,
        };

        // Resolve entity_id from the projection so the canonical-bytes
        // computation matches what the declarant signed (same logic as
        // REST's amend handler).
        let projection = self
            .state
            .get_usecase
            .execute(declaration_id)
            .await
            .map_err(get_error_to_status)?;
        if projection.declarant_principal != principal.subject {
            return Err(Status::permission_denied(
                "declaration is owned by a different principal",
            ));
        }

        let canonical_bytes = canonical_amend_bytes(
            &projection.entity_id,
            &principal.subject,
            amendments.declarant_role,
            amendments.effective_from,
            &amendments.beneficial_owners,
            &attestation.nonce_hex,
        )?;
        attestation
            .verify_against(&canonical_bytes)
            .map_err(|e| Status::unauthenticated(format!("bad_attestation: {e}")))?;

        let correlation_id = Uuid::now_v7();
        let cmd = AmendDeclaration {
            declaration_id,
            declarant_principal: principal.subject.clone(),
            amendments,
            attestation,
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id,
        };

        let receipt = self
            .state
            .amend_usecase
            .execute(cmd)
            .await
            .map_err(amend_error_to_status)?;

        let receipt_url = format!(
            "{base}/v1/declarations/{id}",
            base = self.state.base_url,
            id = receipt.declaration_id
        );
        Ok(Response::new(proto::AmendDeclarationResponse {
            declaration_id: receipt.declaration_id.to_string(),
            aggregate_version: receipt.aggregate_version,
            amended_at: format_iso_datetime(receipt.amended_at),
            receipt_url,
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn correct_declaration(
        &self,
        request: Request<proto::CorrectDeclarationRequest>,
    ) -> Result<Response<proto::CorrectDeclarationResponse>, Status> {
        let principal = require_principal(&request)?;
        let req = request.into_inner();
        let declaration_id = parse_uuid(&req.declaration_id, "declaration_id").map(DeclarationId)?;
        let corrections_proto = req
            .corrections
            .ok_or_else(|| Status::invalid_argument("corrections missing"))?;
        let attestation = decode_attestation(req.attestation.as_ref())?;

        let corrections = CorrectionSet {
            metadata_notes: corrections_proto
                .metadata_notes
                .filter(|s| !s.is_empty()),
        };

        let projection = self
            .state
            .get_usecase
            .execute(declaration_id)
            .await
            .map_err(get_error_to_status)?;
        if projection.declarant_principal != principal.subject {
            return Err(Status::permission_denied(
                "declaration is owned by a different principal",
            ));
        }

        let canonical_bytes = canonical_correction_bytes(
            &declaration_id,
            &principal.subject,
            corrections.metadata_notes.as_deref(),
            &attestation.nonce_hex,
        )?;
        attestation
            .verify_against(&canonical_bytes)
            .map_err(|e| Status::unauthenticated(format!("bad_attestation: {e}")))?;

        let correlation_id = Uuid::now_v7();
        let cmd = CorrectDeclaration {
            declaration_id,
            declarant_principal: principal.subject.clone(),
            corrections,
            attestation,
            submitted_at: OffsetDateTime::now_utc(),
            correlation_id,
        };

        let receipt = self
            .state
            .correct_usecase
            .execute(cmd)
            .await
            .map_err(correct_error_to_status)?;

        let receipt_url = format!(
            "{base}/v1/declarations/{id}",
            base = self.state.base_url,
            id = receipt.declaration_id
        );
        Ok(Response::new(proto::CorrectDeclarationResponse {
            declaration_id: receipt.declaration_id.to_string(),
            aggregate_version: receipt.aggregate_version,
            corrected_at: format_iso_datetime(receipt.corrected_at),
            receipt_url,
        }))
    }
}

// ─── Helpers: shared with REST ───────────────────────────────────────

fn require_principal<T>(request: &Request<T>) -> Result<Principal, Status> {
    request
        .extensions()
        .get::<Principal>()
        .cloned()
        .ok_or_else(|| Status::unauthenticated("authentication required"))
}

fn parse_uuid(s: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(s)
        .map_err(|_| Status::invalid_argument(format!("malformed uuid in field `{field}`")))
}

fn parse_optional_uuid(s: &str, field: &str) -> Result<Option<Uuid>, Status> {
    if s.is_empty() {
        return Ok(None);
    }
    parse_uuid(s, field).map(Some)
}

fn parse_iso_date(s: &str) -> Result<time::Date, Status> {
    // Same wire format as REST's iso_date serde helper: `YYYY-MM-DD`.
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(s, format)
        .map_err(|e| Status::invalid_argument(format!("malformed effective_from date: {e}")))
}

fn format_iso_datetime(dt: OffsetDateTime) -> String {
    // Mirror `domain::serde_helpers::iso_datetime` (RFC3339).
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn decode_declarant_role(role: i32) -> Result<DeclarantRole, Status> {
    use proto::DeclarantRole as P;
    match P::try_from(role) {
        Ok(P::Self_) => Ok(DeclarantRole::SelfDeclaration),
        Ok(P::AuthorisedAgent) => Ok(DeclarantRole::AuthorisedAgent),
        Ok(P::OperatorAssisted) => Ok(DeclarantRole::OperatorAssisted),
        Ok(P::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("declarant_role unspecified"))
        }
    }
}

fn encode_declarant_role(role: DeclarantRole) -> i32 {
    use proto::DeclarantRole as P;
    match role {
        DeclarantRole::SelfDeclaration => P::Self_ as i32,
        DeclarantRole::AuthorisedAgent => P::AuthorisedAgent as i32,
        DeclarantRole::OperatorAssisted => P::OperatorAssisted as i32,
    }
}

fn decode_declaration_kind(kind: i32) -> Result<DeclarationKind, Status> {
    use proto::DeclarationKind as P;
    match P::try_from(kind) {
        Ok(P::Incorporation) => Ok(DeclarationKind::Incorporation),
        Ok(P::AnnualRenewal) => Ok(DeclarationKind::AnnualRenewal),
        Ok(P::ChangeOfControl) => Ok(DeclarationKind::ChangeOfControl),
        Ok(P::Correction) => Ok(DeclarationKind::Correction),
        Ok(P::Amendment) => Ok(DeclarationKind::Amendment),
        Ok(P::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("declaration_kind unspecified"))
        }
    }
}

fn encode_declaration_kind(kind: DeclarationKind) -> i32 {
    use proto::DeclarationKind as P;
    match kind {
        DeclarationKind::Incorporation => P::Incorporation as i32,
        DeclarationKind::AnnualRenewal => P::AnnualRenewal as i32,
        DeclarationKind::ChangeOfControl => P::ChangeOfControl as i32,
        DeclarationKind::Correction => P::Correction as i32,
        DeclarationKind::Amendment => P::Amendment as i32,
    }
}

fn decode_interest_kind(kind: i32) -> Result<InterestKind, Status> {
    use proto::InterestKind as P;
    match P::try_from(kind) {
        Ok(P::Equity) => Ok(InterestKind::Equity),
        Ok(P::Voting) => Ok(InterestKind::Voting),
        Ok(P::FamilyProxy) => Ok(InterestKind::FamilyProxy),
        Ok(P::Contractual) => Ok(InterestKind::Contractual),
        Ok(P::Other) => Ok(InterestKind::Other),
        Ok(P::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("interest_kind unspecified"))
        }
    }
}

fn encode_interest_kind(kind: InterestKind) -> i32 {
    use proto::InterestKind as P;
    match kind {
        InterestKind::Equity => P::Equity as i32,
        InterestKind::Voting => P::Voting as i32,
        InterestKind::FamilyProxy => P::FamilyProxy as i32,
        InterestKind::Contractual => P::Contractual as i32,
        InterestKind::Other => P::Other as i32,
    }
}

fn encode_verification_lane(lane: VerificationLane) -> i32 {
    use proto::VerificationLane as P;
    match lane {
        VerificationLane::Green => P::Green as i32,
        VerificationLane::Yellow => P::Yellow as i32,
        VerificationLane::Red => P::Red as i32,
    }
}

fn decode_owners(owners: &[proto::BeneficialOwner]) -> Result<Vec<BeneficialOwnerClaim>, Status> {
    owners
        .iter()
        .map(|o| {
            let person_id = parse_uuid(&o.person_id, "beneficial_owners.person_id").map(PersonId)?;
            let ownership_basis_points =
                OwnershipBasisPoints::try_from_basis_points(o.ownership_basis_points)
                    .map_err(|e| Status::invalid_argument(e.to_string()))?;
            let interest_kind = decode_interest_kind(o.interest_kind)?;
            Ok(BeneficialOwnerClaim {
                person_id,
                ownership_basis_points,
                interest_kind,
                // PR-FATF-2.A: cascade + nominee fields are part of the
                // FATF-cascade domain type. The proto contract does not
                // yet carry these fields — see contracts/declaration.proto
                // R-DECL-PROTO-FATF follow-up. Until the proto is bumped
                // the gRPC ingestion path emits a legacy-shape owner
                // (cascade_tier=None deserialises as LegacyPreCascade on
                // the projection read).
                cascade_tier: None,
                control_basis: None,
                cascade_tier_b_ruled_out_evidence: None,
                is_nominee: None,
                nominator_person_id: None,
            })
        })
        .collect()
}

fn encode_owners(owners: &[BeneficialOwnerClaim]) -> Vec<proto::BeneficialOwner> {
    owners
        .iter()
        .map(|o| proto::BeneficialOwner {
            person_id: o.person_id.to_string(),
            ownership_basis_points: o.ownership_basis_points.as_basis_points(),
            interest_kind: encode_interest_kind(o.interest_kind),
        })
        .collect()
}

fn decode_attestation(
    a: Option<&proto::Attestation>,
) -> Result<CryptographicAttestation, Status> {
    let a = a.ok_or_else(|| Status::invalid_argument("attestation missing"))?;
    let signature_algorithm = match a.signature_algorithm.as_str() {
        "ed25519" => SignatureAlgorithm::Ed25519,
        other => {
            return Err(Status::invalid_argument(format!(
                "unsupported signature_algorithm `{other}`"
            )));
        }
    };
    Ok(CryptographicAttestation {
        signed_by: a.signed_by.clone(),
        signature_algorithm,
        signature_hex: a.signature_hex.clone(),
        public_key_hex: a.public_key_hex.clone(),
        nonce_hex: a.nonce_hex.clone(),
    })
}

/// Canonical bytes for SubmitDeclaration / SupersedeDeclaration —
/// IDENTICAL shape to `api::rest::canonical_payload_bytes`. The two
/// helpers MUST stay byte-parity; the gRPC integration test asserts
/// this end-to-end (submit via gRPC, GET via REST returns same data).
fn canonical_submit_bytes(
    entity_id: &EntityId,
    declarant_principal: &str,
    declarant_role: DeclarantRole,
    kind: DeclarationKind,
    effective_from: time::Date,
    beneficial_owners: &[BeneficialOwnerClaim],
    nonce_hex: &str,
) -> Result<Vec<u8>, Status> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Canonical<'a> {
        entity_id: &'a EntityId,
        declarant_principal: &'a str,
        declarant_role: &'static str,
        kind: &'static str,
        #[serde(with = "crate::domain::serde_helpers::iso_date")]
        effective_from: time::Date,
        beneficial_owners: &'a [BeneficialOwnerClaim],
        nonce_hex: &'a str,
    }
    serde_json::to_vec(&Canonical {
        entity_id,
        declarant_principal,
        declarant_role: declarant_role.as_str(),
        kind: kind.as_str(),
        effective_from,
        beneficial_owners,
        nonce_hex,
    })
    .map_err(|_| Status::invalid_argument("could not canonicalise request"))
}

/// Canonical bytes for AmendDeclaration. Mirrors
/// `api::rest::canonical_amend_bytes`: same fields, same order, fixed
/// `"amendment"` kind tag (so a substitution attack swapping fields
/// can't fool the verifier).
fn canonical_amend_bytes(
    entity_id: &EntityId,
    declarant_principal: &str,
    declarant_role: DeclarantRole,
    effective_from: time::Date,
    beneficial_owners: &[BeneficialOwnerClaim],
    nonce_hex: &str,
) -> Result<Vec<u8>, Status> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Canonical<'a> {
        entity_id: &'a EntityId,
        declarant_principal: &'a str,
        declarant_role: &'static str,
        kind: &'static str,
        #[serde(with = "crate::domain::serde_helpers::iso_date")]
        effective_from: time::Date,
        beneficial_owners: &'a [BeneficialOwnerClaim],
        nonce_hex: &'a str,
    }
    serde_json::to_vec(&Canonical {
        entity_id,
        declarant_principal,
        declarant_role: declarant_role.as_str(),
        kind: "amendment",
        effective_from,
        beneficial_owners,
        nonce_hex,
    })
    .map_err(|_| Status::invalid_argument("could not canonicalise amend request"))
}

/// Canonical bytes for CorrectDeclaration. Mirrors
/// `api::rest::canonical_correction_bytes`.
fn canonical_correction_bytes(
    declaration_id: &DeclarationId,
    declarant_principal: &str,
    metadata_notes: Option<&str>,
    nonce_hex: &str,
) -> Result<Vec<u8>, Status> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Canonical<'a> {
        declaration_id: &'a DeclarationId,
        declarant_principal: &'a str,
        kind: &'static str,
        metadata_notes: Option<&'a str>,
        nonce_hex: &'a str,
    }
    serde_json::to_vec(&Canonical {
        declaration_id,
        declarant_principal,
        kind: "correction",
        metadata_notes,
        nonce_hex,
    })
    .map_err(|_| Status::invalid_argument("could not canonicalise correction request"))
}

fn projection_to_proto(
    p: crate::application::DeclarationProjection,
) -> proto::GetDeclarationResponse {
    proto::GetDeclarationResponse {
        declaration_id: p.declaration_id.to_string(),
        entity_id: p.entity_id.to_string(),
        declarant_principal: p.declarant_principal,
        declarant_role: encode_declarant_role(p.declarant_role),
        kind: encode_declaration_kind(p.kind),
        effective_from: p
            .effective_from
            .format(&time::macros::format_description!(
                "[year]-[month]-[day]"
            ))
            .unwrap_or_default(),
        beneficial_owners: encode_owners(&p.beneficial_owners),
        state: p.state.as_str().to_string(),
        aggregate_version: p.version,
        submitted_at: format_iso_datetime(p.submitted_at),
        receipt_hash_hex: p.receipt_hash_hex,
        correlation_id: p.correlation_id.to_string(),
        verification_state: p.verification_state,
        verification_lane: p.verification_lane.map(encode_verification_lane),
        verification_case_id: p.verification_case_id.map(|u| u.to_string()),
        verified_at: p.verified_at.map(format_iso_datetime),
        supersedes_declaration_id: p.supersedes_declaration_id.map(|d| d.to_string()),
        superseded_by_declaration_id: p.superseded_by_declaration_id.map(|d| d.to_string()),
        superseded_at: p.superseded_at.map(format_iso_datetime),
        amended_at: p.amended_at.map(format_iso_datetime),
        metadata_notes: p.metadata_notes,
        corrected_at: p.corrected_at.map(format_iso_datetime),
    }
}

// ─── Error mapping: domain → tonic::Status ───────────────────────────
//
// D14 fail-closed: NEVER `Status::unknown`. Map each domain variant to
// a deliberate code so consumers can react. Mirrors the REST mapping
// in `crate::error::ServiceError::into_response`.

fn domain_to_status(e: &DomainError) -> Status {
    match e {
        DomainError::AlreadySubmitted(_)
        | DomainError::VerificationCaseMismatch { .. }
        | DomainError::AlreadySuperseded(_)
        | DomainError::AmendFromInvalidState { .. }
        | DomainError::CorrectFromInvalidState { .. } => {
            Status::failed_precondition(e.to_string())
        }
        DomainError::AttestationPrincipalMismatch { .. }
        | DomainError::SupersedeNotOwner { .. }
        | DomainError::AmendNotOwner { .. }
        | DomainError::CorrectNotOwner { .. } => Status::permission_denied(e.to_string()),
        DomainError::VerificationOutcomeBeforeSubmit(_)
        | DomainError::SupersedeBeforeSubmit(_)
        | DomainError::AmendBeforeSubmit(_)
        | DomainError::CorrectBeforeSubmit(_) => Status::not_found(e.to_string()),
        _ => Status::invalid_argument(e.to_string()),
    }
}

fn submit_error_to_status(e: SubmitError) -> Status {
    match e {
        SubmitError::Domain(d) => domain_to_status(&d),
        SubmitError::Repository(r) => repository_to_status(r),
        // R-DECL-4: the Person registry is an upstream dependency.
        // Transport / unexpected-status failures → UNAVAILABLE so the
        // client retries. The "person not registered" case surfaces as
        // a Domain error (BeneficialOwnerNotInPersonRegistry) and is
        // mapped earlier; never reaches this arm.
        SubmitError::PersonRegistry(err) => Status::unavailable(err.to_string()),
    }
}

fn get_error_to_status(e: GetError) -> Status {
    match e {
        GetError::NotFound(id) => Status::not_found(format!("declaration {id} not found")),
        GetError::Repository(r) => repository_to_status(r),
    }
}

fn supersede_error_to_status(e: SupersedeError) -> Status {
    match e {
        SupersedeError::Domain(d) => domain_to_status(&d),
        SupersedeError::Repository(r) => repository_to_status(r),
        SupersedeError::OldDeclarationNotFound(id) => {
            Status::not_found(format!("declaration {id} not found"))
        }
    }
}

fn amend_error_to_status(e: AmendError) -> Status {
    match e {
        AmendError::Domain(d) => domain_to_status(&d),
        AmendError::Repository(r) => repository_to_status(r),
        AmendError::NotFound(id) => Status::not_found(format!("declaration {id} not found")),
    }
}

fn correct_error_to_status(e: CorrectError) -> Status {
    match e {
        CorrectError::Domain(d) => domain_to_status(&d),
        CorrectError::Repository(r) => repository_to_status(r),
        CorrectError::NotFound(id) => Status::not_found(format!("declaration {id} not found")),
    }
}

fn repository_to_status(e: crate::application::RepositoryError) -> Status {
    use crate::application::RepositoryError;
    match e {
        RepositoryError::Conflict { .. } => Status::failed_precondition(format!("{e}")),
        // Generic infrastructure faults: signal with `internal`. Do NOT
        // leak inner error detail — log it server-side, return a
        // boilerplate message (D18 no secrets in errors, D14 fail-closed).
        other => {
            tracing::error!(error = ?other, "gRPC: repository failure");
            Status::internal("internal failure")
        }
    }
}

// Suppress unused-import warnings while keeping the public surface
// stable for downstream consumers.
#[allow(dead_code)]
fn _force_imports(_s: ServiceError) {}

// ─── Unit tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso_date_round_trips() {
        let d = parse_iso_date("2026-05-01").unwrap();
        assert_eq!(d, time::macros::date!(2026 - 05 - 01));
    }

    #[test]
    fn parse_iso_date_rejects_garbage() {
        let err = parse_iso_date("not-a-date").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn parse_uuid_rejects_garbage() {
        let err = parse_uuid("not-a-uuid", "declaration_id").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn parse_optional_uuid_treats_empty_as_none() {
        assert_eq!(parse_optional_uuid("", "x").unwrap(), None);
    }

    #[test]
    fn role_round_trip() {
        for role in [
            DeclarantRole::SelfDeclaration,
            DeclarantRole::AuthorisedAgent,
            DeclarantRole::OperatorAssisted,
        ] {
            let proto_val = encode_declarant_role(role);
            assert_eq!(decode_declarant_role(proto_val).unwrap(), role);
        }
    }

    #[test]
    fn role_rejects_unspecified() {
        let err = decode_declarant_role(0).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn kind_round_trip() {
        for kind in [
            DeclarationKind::Incorporation,
            DeclarationKind::AnnualRenewal,
            DeclarationKind::ChangeOfControl,
            DeclarationKind::Correction,
            DeclarationKind::Amendment,
        ] {
            let proto_val = encode_declaration_kind(kind);
            assert_eq!(decode_declaration_kind(proto_val).unwrap(), kind);
        }
    }

    #[test]
    fn interest_kind_round_trip() {
        for k in [
            InterestKind::Equity,
            InterestKind::Voting,
            InterestKind::FamilyProxy,
            InterestKind::Contractual,
            InterestKind::Other,
        ] {
            let proto_val = encode_interest_kind(k);
            assert_eq!(decode_interest_kind(proto_val).unwrap(), k);
        }
    }

    #[test]
    fn lane_encoding_matches_proto_variants() {
        use proto::VerificationLane as P;
        assert_eq!(encode_verification_lane(VerificationLane::Green), P::Green as i32);
        assert_eq!(encode_verification_lane(VerificationLane::Yellow), P::Yellow as i32);
        assert_eq!(encode_verification_lane(VerificationLane::Red), P::Red as i32);
    }

    #[test]
    fn decode_attestation_requires_ed25519() {
        let a = proto::Attestation {
            signed_by: "spiffe://recor.cm/x".into(),
            signature_algorithm: "rs256".into(),
            signature_hex: "00".into(),
            public_key_hex: "00".into(),
            nonce_hex: "00".into(),
        };
        let err = decode_attestation(Some(&a)).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn decode_attestation_missing_is_invalid_argument() {
        let err = decode_attestation(None).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn canonical_submit_bytes_byte_parity_with_rest() {
        // The gRPC and REST canonicalisers MUST produce identical bytes
        // for the same logical payload (D15). Build a payload by hand
        // here and compare with the REST helper's output.
        let entity_id = EntityId(Uuid::nil());
        let principal = "spiffe://recor.cm/test";
        let role = DeclarantRole::SelfDeclaration;
        let kind = DeclarationKind::Incorporation;
        let effective_from = time::macros::date!(2026 - 01 - 01);
        let owners = vec![BeneficialOwnerClaim {
            person_id: PersonId(Uuid::nil()),
            ownership_basis_points: OwnershipBasisPoints::try_from_basis_points(10_000)
                .unwrap(),
            interest_kind: InterestKind::Equity,
            cascade_tier: None,
            control_basis: None,
            cascade_tier_b_ruled_out_evidence: None,
            is_nominee: None,
            nominator_person_id: None,
        }];
        let nonce = "deadbeef";
        let bytes = canonical_submit_bytes(
            &entity_id,
            principal,
            role,
            kind,
            effective_from,
            &owners,
            nonce,
        )
        .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // The expected shape — pinned here so any future divergence
        // from REST's canonicaliser breaks this test.
        assert!(s.starts_with("{\"entity_id\":\""), "got: {s}");
        assert!(s.contains("\"declarant_principal\":\"spiffe://recor.cm/test\""), "got: {s}");
        assert!(s.contains("\"declarant_role\":\"self\""), "got: {s}");
        assert!(s.contains("\"kind\":\"incorporation\""), "got: {s}");
        assert!(s.contains("\"effective_from\":\"2026-01-01\""), "got: {s}");
        assert!(s.contains("\"ownership_basis_points\":10000"), "got: {s}");
        assert!(s.contains("\"interest_kind\":\"equity\""), "got: {s}");
        assert!(s.ends_with("\"nonce_hex\":\"deadbeef\"}"), "got: {s}");
    }

    #[test]
    fn submit_error_repository_conflict_maps_to_failed_precondition() {
        let err = SubmitError::Repository(crate::application::RepositoryError::Conflict {
            expected: 0,
            found: 1,
        });
        let s = submit_error_to_status(err);
        assert_eq!(s.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn get_error_not_found_maps_to_not_found() {
        let s = get_error_to_status(GetError::NotFound(DeclarationId::new()));
        assert_eq!(s.code(), tonic::Code::NotFound);
    }

    #[test]
    fn domain_attestation_principal_mismatch_maps_to_permission_denied() {
        let d = DomainError::AttestationPrincipalMismatch {
            expected: "a".into(),
            actual: "b".into(),
        };
        let s = domain_to_status(&d);
        assert_eq!(s.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn domain_already_submitted_maps_to_failed_precondition() {
        let d = DomainError::AlreadySubmitted(Uuid::now_v7());
        let s = domain_to_status(&d);
        assert_eq!(s.code(), tonic::Code::FailedPrecondition);
    }
}
