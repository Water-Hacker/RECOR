//! `OpenAPI` 3.1 spec assembly + the `/openapi.json` and `/docs` routes
//! (DOC-1).
//!
//! The spec is built from the `#[utoipa::path]` annotations in
//! `api::rest`, `api::dlq`, and `api::internal`, plus the `ToSchema`
//! annotations on every wire-level DTO in `api::dto` and on the domain
//! value-objects under `domain::value_object` / `domain::attestation`.
//!
//! Three rules that future maintainers must follow:
//!
//!   1. **Don't ship a partial spec.** D01 (completeness): a partial
//!      spec is worse than no spec because consumers assume it's
//!      authoritative. If you add a new handler, add the
//!      `#[utoipa::path]` annotation in the same PR.
//!
//!   2. **The spec is the docs.** D05 (documentation is part of the
//!      feature): per-endpoint descriptions live in the
//!      `#[utoipa::path]` attribute, not in a separate README. Same
//!      for DTO field-level docs (the rustdoc `///` comments above
//!      each field become the `description` in the spec).
//!
//!   3. **Auth requirements are documented.** D17 (zero trust):
//!      every authenticated endpoint declares its security scheme
//!      via `security(...)`. Three schemes are defined here:
//!      `bearerAuth` (OIDC JWT — the production path),
//!      `devPrincipalHeader` (the dev-only `X-Recor-Dev-Principal`
//!      header), and `hmacSignature` (HMAC-signed inbound webhook
//!      from the verification engine on
//!      `POST /v1/internal/verification-outcomes`).
//!
//! The committed snapshot lives at `docs/openapi/declaration.json`;
//! `tools/ci/check-openapi-drift.sh` enforces that the snapshot matches
//! what the build produces.

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use utoipa::openapi::security::{
    ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme,
};
use utoipa::{Modify, OpenApi};
use utoipa_scalar::{Scalar, Servable};

use crate::api::dlq::{self, DlqItem, ListDlqResponse, ReplayDlqResponse};
use crate::api::dto::{
    ErrorBody, ErrorEnvelope, GetDeclarationResponse, HealthzResponse, ReadyzResponse,
    SubmitDeclarationRequest, SubmitDeclarationResponse, SupersedeDeclarationResponse,
    VerificationOutcomeRequest, VerificationOutcomeResponse,
};
use crate::api::internal;
use crate::api::rest;
use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
use crate::domain::value_object::InterestKind;
use crate::domain::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, DeclarationState,
    EntityId, OwnershipBasisPoints, PersonId, VerificationLane,
};

/// Marker struct that carries the `#[derive(OpenApi)]` document.
/// Generating the spec is a pure operation — call
/// `ApiDoc::openapi()` and you get a fresh `utoipa::openapi::OpenApi`.
///
/// Keep the `paths(...)` list and the `components(schemas(...))` list
/// in sync with the handlers/DTOs. The unit tests in this module
/// assert that the well-known endpoints + schemas show up in the
/// resulting JSON.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "RÉCOR Declaration Service",
        version = env!("CARGO_PKG_VERSION"),
        description = "REST surface of the RÉCOR Declaration service. \
            Accepts beneficial-ownership declarations, returns signed receipts, \
            relays verification outcomes from the Verification Engine, and \
            exposes operator endpoints under `/v1/internal/*`. \
            Endpoints under the `internal` tag are not for general consumer use.",
        license(name = "Apache-2.0"),
        contact(name = "RÉCOR domain team", email = "noreply@recor.cm"),
    ),
    servers(
        (url = "http://localhost:8080", description = "Local dev"),
        (url = "https://api.recor.cm", description = "Production (TBD)"),
    ),
    tags(
        (name = "declarations", description = "Beneficial-ownership declaration intake and lookup."),
        (name = "system", description = "Liveness and readiness probes."),
        (name = "internal", description = "Operator and internal-service endpoints. Not intended for general consumption; subject to change."),
    ),
    paths(
        rest::healthz,
        rest::readyz,
        rest::submit_declaration,
        rest::get_declaration,
        rest::supersede_declaration,
        dlq::list_dlq,
        dlq::replay_dlq,
        internal::handle_verification_outcome,
    ),
    components(
        schemas(
            // Public DTOs
            SubmitDeclarationRequest,
            SubmitDeclarationResponse,
            GetDeclarationResponse,
            SupersedeDeclarationResponse,
            VerificationOutcomeRequest,
            VerificationOutcomeResponse,
            // System DTOs
            HealthzResponse,
            ReadyzResponse,
            // Cross-cutting envelopes
            ErrorEnvelope,
            ErrorBody,
            // Internal/admin DTOs
            ListDlqResponse,
            DlqItem,
            ReplayDlqResponse,
            // Domain value objects
            DeclarationId,
            EntityId,
            PersonId,
            OwnershipBasisPoints,
            DeclarantRole,
            DeclarationKind,
            DeclarationState,
            VerificationLane,
            BeneficialOwnerClaim,
            InterestKind,
            CryptographicAttestation,
            SignatureAlgorithm,
        ),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

/// Security-scheme registration. `bearerAuth` is the production OIDC
/// path; `devPrincipalHeader` is the dev-only header shortcut;
/// `hmacSignature` covers the HMAC-signed webhook from the Verification
/// Engine. None of these are applied globally — each handler explicitly
/// declares which schemes it accepts via the `security(...)` attribute.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().expect(
            "openapi components should have been initialised by the derive",
        );
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "OIDC-issued JWT. The service verifies the signature against the configured issuer's JWKS, plus `iss`/`aud`/`exp`/`nbf` claims. HMAC algorithms are refused (algorithm-confusion mitigation).",
                    ))
                    .build(),
            ),
        );
        components.add_security_scheme(
            "devPrincipalHeader",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Recor-Dev-Principal",
                "Dev-only principal shortcut. Refused outside ENVIRONMENT=dev. Production must always use `bearerAuth`.",
            ))),
        );
        components.add_security_scheme(
            "hmacSignature",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-RECOR-Signature",
                "HMAC-SHA256 over the raw request body, hex-encoded, using the shared writeback secret. Constant-time verification; rotation supported via a transient old-secret window.",
            ))),
        );
    }
}

/// Public entry point: build the `OpenAPI` document. Pure function;
/// callers may serialise it to JSON or YAML.
pub fn build_openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// Mount the `OpenAPI` artefact routes onto a router. Returns a router
/// with `GET /openapi.json` and the Scalar UI at `GET /docs`. Neither
/// route requires authentication — the spec is a public contract.
pub fn openapi_routes() -> Router {
    let spec = build_openapi();
    // `utoipa-scalar` provides `From<Scalar<S>> for Router<R>`; spell
    // out the target type so axum doesn't have to infer the state
    // generic. The result is a `Router<()>` carrying just the `/docs`
    // GET. We merge it with our own `/openapi.json` route to produce
    // the artefact-only sub-router.
    let scalar: Router = Scalar::with_url("/docs", spec).into();
    Router::new()
        .route("/openapi.json", get(serve_openapi_json))
        .merge(scalar)
}

/// Returns the `OpenAPI` document as JSON. The body is computed once at
/// router-build time (`build_openapi()`); we materialise it here on
/// each request so changes during tests can still be observed.
async fn serve_openapi_json() -> impl IntoResponse {
    let body = serde_json::to_string(&build_openapi())
        .unwrap_or_else(|_| "{\"error\":\"openapi serialise failed\"}".to_string());
    (
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
}

#[cfg(test)]
mod tests {
    //! Regression guard for the `OpenAPI` spec.
    //!
    //! These tests are deliberately schema-shape assertions, not
    //! byte-equality with the committed snapshot — that is the drift
    //! check (`tools/ci/check-openapi-drift.sh`)'s job. Here we only
    //! verify that handlers we expect to be in the spec are present
    //! and that their well-known fields are described.

    use super::*;
    use serde_json::Value;

    fn doc_json() -> Value {
        serde_json::to_value(build_openapi()).expect("spec serialises to JSON")
    }

    #[test]
    fn openapi_is_3_1() {
        let v = doc_json();
        assert_eq!(
            v["openapi"].as_str().expect("openapi field"),
            "3.1.0",
            "spec must declare openapi 3.1.0"
        );
    }

    #[test]
    fn covers_every_public_path() {
        let v = doc_json();
        let paths = v["paths"]
            .as_object()
            .expect("paths object present");
        for expected in [
            "/healthz",
            "/readyz",
            "/v1/declarations",
            "/v1/declarations/{declaration_id}",
            "/v1/declarations/{declaration_id}/supersede",
            "/v1/internal/outbox-dlq",
            "/v1/internal/outbox-dlq/{id}/replay",
            "/v1/internal/verification-outcomes",
        ] {
            assert!(
                paths.contains_key(expected),
                "missing path {expected} in spec; paths present: {:?}",
                paths.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn submit_endpoint_declares_request_and_known_responses() {
        let v = doc_json();
        let op = &v["paths"]["/v1/declarations"]["post"];
        assert_eq!(op["operationId"], "submitDeclaration");
        let request_body = &op["requestBody"];
        assert!(
            !request_body.is_null(),
            "POST /v1/declarations must declare a requestBody"
        );
        for status in ["201", "400", "401", "409"] {
            assert!(
                !op["responses"][status].is_null(),
                "expected {status} response on submit_declaration; got responses: {:?}",
                op["responses"]
            );
        }
        // Must reference the standard error envelope on 4xx.
        let four_oh_one_ref = op["responses"]["401"]["content"]["application/json"]
            ["schema"]["$ref"]
            .as_str()
            .unwrap_or_default();
        assert!(
            four_oh_one_ref.ends_with("/ErrorEnvelope")
                || four_oh_one_ref.ends_with("/ErrorBody"),
            "401 on submit_declaration should reference the error envelope; got {four_oh_one_ref}"
        );
    }

    #[test]
    fn key_schemas_declared() {
        let v = doc_json();
        let schemas = v["components"]["schemas"]
            .as_object()
            .expect("components.schemas object");
        for expected in [
            "SubmitDeclarationRequest",
            "SubmitDeclarationResponse",
            "GetDeclarationResponse",
            "SupersedeDeclarationResponse",
            "VerificationOutcomeRequest",
            "VerificationOutcomeResponse",
            "ErrorEnvelope",
            "ErrorBody",
            "HealthzResponse",
            "ReadyzResponse",
            "ListDlqResponse",
            "DlqItem",
            "ReplayDlqResponse",
            "CryptographicAttestation",
            "BeneficialOwnerClaim",
            "VerificationLane",
            "DeclarationKind",
            "DeclarantRole",
        ] {
            assert!(
                schemas.contains_key(expected),
                "missing schema {expected}; have: {:?}",
                schemas.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn security_schemes_are_registered() {
        let v = doc_json();
        let security_schemes = &v["components"]["securitySchemes"];
        for expected in ["bearerAuth", "devPrincipalHeader", "hmacSignature"] {
            assert!(
                !security_schemes[expected].is_null(),
                "missing security scheme {expected}"
            );
        }
    }

    #[test]
    fn internal_endpoints_carry_internal_tag() {
        let v = doc_json();
        for path in [
            "/v1/internal/outbox-dlq",
            "/v1/internal/outbox-dlq/{id}/replay",
            "/v1/internal/verification-outcomes",
        ] {
            let path_obj = v["paths"][path]
                .as_object()
                .unwrap_or_else(|| panic!("path {path} present"));
            for (_method, op) in path_obj {
                let tags = op["tags"]
                    .as_array()
                    .unwrap_or_else(|| panic!("tags array on {path}"));
                let has_internal = tags
                    .iter()
                    .any(|t| t.as_str() == Some("internal"));
                assert!(
                    has_internal,
                    "{path} must be tagged `internal`; got {tags:?}"
                );
            }
        }
    }

    #[test]
    fn submit_declaration_request_carries_known_fields() {
        // Schema-shape regression: the published request body must
        // expose the fields hand-written code relies on (the portal's
        // generated TS client reads these). If a field is renamed in
        // the DTO and forgotten here, this test catches it.
        let v = doc_json();
        let schema = &v["components"]["schemas"]["SubmitDeclarationRequest"];
        let props = schema["properties"]
            .as_object()
            .expect("SubmitDeclarationRequest.properties");
        for field in [
            "entity_id",
            "declarant_role",
            "kind",
            "effective_from",
            "beneficial_owners",
            "attestation",
        ] {
            assert!(
                props.contains_key(field),
                "SubmitDeclarationRequest missing field {field}; have {:?}",
                props.keys().collect::<Vec<_>>()
            );
        }
    }
}
