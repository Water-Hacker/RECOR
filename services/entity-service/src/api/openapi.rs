//! OpenAPI 3.1 spec assembly + the `/openapi.json` and `/docs` routes.
//!
//! The spec is built from the `#[utoipa::path]` annotations in
//! `api::rest`, plus the `ToSchema` annotations on every wire-level
//! DTO in `api::dto` and on the domain value-objects.
//!
//! Three rules that future maintainers must follow:
//!
//!   1. D01 — don't ship a partial spec. New handler ⇒ new
//!      `#[utoipa::path]` in the same PR.
//!   2. D05 — the spec is the docs. Per-endpoint descriptions live in
//!      the `#[utoipa::path]` attribute; per-field docs live in rustdoc
//!      comments above each DTO field.
//!   3. D17 — every authenticated endpoint declares its security scheme.
//!
//! The committed snapshot lives at `docs/openapi/entity-service.json`;
//! `tools/ci/check-openapi-drift.sh` enforces that the snapshot matches
//! what the build produces.
//!
//! The Prometheus exposition endpoint at `GET /metrics` is intentionally
//! NOT enumerated below (same rationale as recor-declaration).

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_scalar::{Scalar, Servable};

use crate::api::dlq::{self, DlqItem, ListDlqResponse, ReplayDlqResponse};
use crate::api::dto::{
    DissolveEntityRequest, DissolveResponse, EntityTypeDto, ErrorBody, ErrorEnvelope,
    GetEntityResponse, HealthzResponse, ReadyzResponse, RegisterEntityRequest,
    RegisterEntityResponse, SearchEntitiesResponse, UpdateEntityRequest, UpdateResponse,
};
use crate::api::rest;
use crate::domain::value_object::{EntityType, Jurisdiction, RegistrationNumber};
use crate::domain::EntityId;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "RÉCOR Entity Service",
        version = env!("CARGO_PKG_VERSION"),
        description = "REST surface of the RÉCOR Entity service (IDENTITY-1). \
            Authoritative cache + projection of legal-entity registry data \
            for Cameroon and other jurisdictions. Entities are Public-classified \
            at the projection layer; no PII columns. Once R-VER-1 ships, this \
            service becomes the authoritative cache of BUNEC entries for \
            Cameroon entities.",
        license(name = "Apache-2.0"),
        contact(name = "RÉCOR domain team", email = "noreply@recor.cm"),
    ),
    servers(
        (url = "http://localhost:8083", description = "Local dev"),
        (url = "https://api.recor.cm", description = "Production (TBD)"),
    ),
    tags(
        (name = "entities", description = "Legal-entity registration, projection, and lifecycle."),
        (name = "system", description = "Liveness and readiness probes."),
        (name = "internal", description = "Operator-only DLQ administration. Admin-allowlist gated; refuses when ADMIN_PRINCIPALS is empty (D17 + D14)."),
    ),
    paths(
        rest::healthz,
        rest::readyz,
        rest::register_entity,
        rest::get_entity,
        rest::search_entities,
        rest::update_entity_handler,
        rest::dissolve_entity_handler,
        dlq::list_dlq,
        dlq::replay_dlq,
    ),
    components(
        schemas(
            // Public DTOs
            RegisterEntityRequest,
            RegisterEntityResponse,
            UpdateEntityRequest,
            UpdateResponse,
            DissolveEntityRequest,
            DissolveResponse,
            GetEntityResponse,
            SearchEntitiesResponse,
            // System DTOs
            HealthzResponse,
            ReadyzResponse,
            // DLQ admin DTOs (TODO-039)
            ListDlqResponse,
            DlqItem,
            ReplayDlqResponse,
            // Cross-cutting envelopes
            ErrorEnvelope,
            ErrorBody,
            // Domain value objects
            EntityId,
            Jurisdiction,
            RegistrationNumber,
            EntityType,
            EntityTypeDto,
        ),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("openapi components should have been initialised by the derive");
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "OIDC-issued JWT. The service verifies the signature against the configured issuer's JWKS, plus `iss`/`aud`/`exp`/`nbf` claims. HMAC algorithms are refused.",
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
    }
}

pub fn build_openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

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
    use super::*;
    use serde_json::Value;

    fn doc_json() -> Value {
        serde_json::to_value(build_openapi()).expect("spec serialises to JSON")
    }

    #[test]
    fn openapi_lists_register_endpoint() {
        let v = doc_json();
        let paths = v.get("paths").expect("paths object");
        assert!(paths.get("/v1/entities").is_some(), "missing POST /v1/entities");
        assert!(paths.get("/v1/entities/{entity_id}").is_some());
        assert!(paths.get("/v1/entities/search").is_some());
        assert!(paths.get("/v1/entities/{entity_id}/dissolve").is_some());
    }

    #[test]
    fn openapi_carries_bearer_security_scheme() {
        let v = doc_json();
        let schemes = v
            .pointer("/components/securitySchemes")
            .expect("securitySchemes present");
        assert!(schemes.get("bearerAuth").is_some());
        assert!(schemes.get("devPrincipalHeader").is_some());
    }
}
