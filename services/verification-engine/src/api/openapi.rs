//! `OpenAPI` 3.1 spec assembly + the `/openapi.json` and `/docs` routes
//! for the verification engine (DOC-1 mirror; FIND-013 closure).
//!
//! The spec is built from the `#[utoipa::path]` annotations in
//! `api::rest`, `api::dlq`, and `api::internal`, plus the `ToSchema`
//! annotations on the wire DTOs. Deep nested domain types
//! (`DeclarationSnapshot`, `VerificationCase`) are pinned via
//! `serde_json::Value` in the spec — the authoritative schema for
//! those bodies lives in `services/declaration`'s OpenAPI document.
//!
//! Three rules future maintainers must follow (same as declaration):
//!
//!   1. **Don't ship a partial spec.** D01 (completeness): a partial
//!      spec is worse than no spec because consumers assume it's
//!      authoritative. If you add a new handler, add the
//!      `#[utoipa::path]` annotation in the same PR.
//!   2. **The spec is the docs.** D05: per-endpoint descriptions live
//!      in `#[utoipa::path]`; rustdoc `///` comments on DTO fields
//!      become the OpenAPI `description`.
//!   3. **Auth requirements are documented.** D17: every authenticated
//!      endpoint declares its security scheme. Three schemes are
//!      defined: `bearerAuth`, `devPrincipalHeader`, `hmacSignature`.
//!
//! # OBS-1: operational endpoints are NOT in the consumer contract
//!
//! `/metrics` is intentionally NOT enumerated in `paths(...)` — its
//! shape follows the Prometheus text-exposition format and it is
//! served on a separate listener (FIND-007). Including it in the
//! consumer-facing spec would (a) imply it is part of the contract
//! and (b) leak the existence of an internal endpoint into a public
//! document.

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
use crate::api::internal::{self, InboundResponse};
use crate::api::rest::{
    self, ErrorBody, ErrorEnvelope, HealthzResponse, ReadyzResponse,
    SubmitVerificationRequest, SubmitVerificationResponse,
};

/// Marker struct carrying the `#[derive(OpenApi)]` document.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "RÉCOR Verification Engine",
        version = env!("CARGO_PKG_VERSION"),
        description = "REST surface of the RÉCOR Verification Engine. \
            Operator-only `submit_verification` (the production \
            verification-submission path is the HMAC-authenticated \
            internal webhook), declarant-owner-or-admin gated \
            `get_verification`, DLQ admin endpoints, and the \
            HMAC-authenticated declaration-events inbound webhook. \
            Endpoints under the `internal` tag are not for general \
            consumer use.",
        license(name = "Apache-2.0"),
        contact(name = "RÉCOR verification team", email = "noreply@recor.cm"),
    ),
    servers(
        (url = "http://localhost:8081", description = "Local dev"),
        (url = "https://api.recor.cm", description = "Production (TBD)"),
    ),
    tags(
        (name = "verifications", description = "Verification case submit + read."),
        (name = "system", description = "Liveness and readiness probes."),
        (name = "internal", description = "Operator and internal-service endpoints. Not intended for general consumption; subject to change."),
    ),
    paths(
        rest::healthz,
        rest::readyz,
        rest::submit_verification,
        rest::get_verification,
        rest::get_verification_rationale,
        dlq::list_dlq,
        dlq::replay_dlq,
        internal::handle_declaration_event,
    ),
    components(
        schemas(
            // Public DTOs
            SubmitVerificationRequest,
            SubmitVerificationResponse,
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
            InboundResponse,
        ),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

/// Security-scheme registration. Mirrors the declaration service's
/// schemes so a consumer that already integrates with declaration can
/// reuse its auth layer here.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("openapi components initialised by derive");
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
                "HMAC-SHA256 over the raw request body, hex-encoded, using the shared inbound secret. Constant-time verification; rotation supported via a transient old-secret window.",
            ))),
        );
    }
}

/// Build the `OpenAPI` document. Pure function.
pub fn build_openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// Mount `GET /openapi.json` + the Scalar UI at `GET /docs`.
pub fn openapi_routes() -> Router {
    let spec = build_openapi();
    let scalar: Router = Scalar::with_url("/docs", spec).into();
    Router::new()
        .route("/openapi.json", get(serve_openapi_json))
        .merge(scalar)
}

async fn serve_openapi_json() -> impl IntoResponse {
    let body = serde_json::to_string(&build_openapi())
        .unwrap_or_else(|_| "{\"error\":\"openapi serialise failed\"}".to_string());
    ([(header::CONTENT_TYPE, "application/json")], body)
}

#[cfg(test)]
mod tests {
    //! Schema-shape regression tests. The drift check
    //! (`tools/ci/check-openapi-drift.sh`) is the byte-equality guard;
    //! these tests assert structural invariants so a regression
    //! surfaces at the unit level.

    use super::*;
    use serde_json::Value;

    fn doc_json() -> Value {
        serde_json::to_value(build_openapi()).expect("spec serialises to JSON")
    }

    #[test]
    fn openapi_is_3_1() {
        let v = doc_json();
        assert_eq!(v["openapi"].as_str().expect("openapi field"), "3.1.0");
    }

    #[test]
    fn every_public_path_present() {
        let v = doc_json();
        let paths = v["paths"].as_object().expect("paths object");
        for expected in [
            "/healthz",
            "/readyz",
            "/v1/verifications",
            "/v1/verifications/{case_id}",
            "/v1/verifications/{case_id}/rationale",
            "/v1/internal/verification-outbox-dlq",
            "/v1/internal/verification-outbox-dlq/{id}/replay",
            "/v1/internal/declaration-events",
        ] {
            assert!(
                paths.contains_key(expected),
                "missing path {expected}; have {:?}",
                paths.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn submit_endpoint_declares_request_and_known_responses() {
        let v = doc_json();
        let op = &v["paths"]["/v1/verifications"]["post"];
        assert_eq!(op["operationId"], "submitVerification");
        assert!(
            !op["requestBody"].is_null(),
            "POST /v1/verifications must declare a requestBody"
        );
        for status in ["201", "401", "403", "503"] {
            assert!(
                !op["responses"][status].is_null(),
                "expected {status} on submit_verification; got {:?}",
                op["responses"]
            );
        }
    }

    #[test]
    fn get_endpoint_documents_404_for_cross_tenant_denial() {
        let v = doc_json();
        let op = &v["paths"]["/v1/verifications/{case_id}"]["get"];
        assert_eq!(op["operationId"], "getVerification");
        assert!(
            !op["responses"]["404"].is_null(),
            "FIND-004: cross-tenant denial must surface as 404 in the spec"
        );
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
            "/v1/internal/verification-outbox-dlq",
            "/v1/internal/verification-outbox-dlq/{id}/replay",
            "/v1/internal/declaration-events",
        ] {
            let path_obj = v["paths"][path]
                .as_object()
                .unwrap_or_else(|| panic!("path {path} present"));
            for (_method, op) in path_obj {
                let tags = op["tags"]
                    .as_array()
                    .unwrap_or_else(|| panic!("tags array on {path}"));
                let has_internal = tags.iter().any(|t| t.as_str() == Some("internal"));
                assert!(has_internal, "{path} must be tagged internal; got {tags:?}");
            }
        }
    }

    #[test]
    fn metrics_endpoint_is_intentionally_absent() {
        let v = doc_json();
        let paths = v["paths"].as_object().expect("paths object");
        assert!(
            !paths.contains_key("/metrics"),
            "/metrics is operational (OBS-1) and must NOT appear in the consumer-facing spec"
        );
    }
}
