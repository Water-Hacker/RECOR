//! OpenAPI 3.1 spec assembly + the `/openapi.json` and `/docs` routes.

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use utoipa::openapi::security::{
    ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme,
};
use utoipa::{Modify, OpenApi};
use utoipa_scalar::{Scalar, Servable};

use crate::api::dto::{
    ErrorBody, ErrorEnvelope, GetPersonResponse, HealthzResponse, MergePersonsResponse,
    ReadyzResponse, RegisterPersonRequest, RegisterPersonResponse, SearchPersonsResponse,
};
use crate::api::rest;
use crate::application::SearchQuery;
use crate::domain::value_object::{
    CanonicalFullName, IdDocument, IdDocumentType, Nationality, PersonAttributes, PersonId,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "RÉCOR Person Service",
        version = env!("CARGO_PKG_VERSION"),
        description = "REST surface of the RÉCOR Person service. \
            Canonical natural-person registry; anchors every `person_id` \
            referenced inside Declaration-service beneficial-owner claims. \
            v1 covers register, get, search, and operator-only merge. \
            NDI integration is deferred (see service CLAUDE.md).",
        license(name = "Apache-2.0"),
        contact(name = "RÉCOR domain team", email = "noreply@recor.cm"),
    ),
    servers(
        (url = "http://localhost:8082", description = "Local dev"),
        (url = "https://api.recor.cm", description = "Production (TBD)"),
    ),
    tags(
        (name = "persons", description = "Person registry intake, lookup, and merge."),
        (name = "system", description = "Liveness and readiness probes."),
    ),
    paths(
        rest::healthz,
        rest::readyz,
        rest::register_person,
        rest::get_person,
        rest::search_persons,
        rest::merge_persons,
    ),
    components(
        schemas(
            // Public DTOs
            RegisterPersonRequest,
            RegisterPersonResponse,
            GetPersonResponse,
            SearchPersonsResponse,
            MergePersonsResponse,
            SearchQuery,
            // System DTOs
            HealthzResponse,
            ReadyzResponse,
            // Error envelopes
            ErrorEnvelope,
            ErrorBody,
            // Domain value objects
            PersonId,
            PersonAttributes,
            CanonicalFullName,
            Nationality,
            IdDocument,
            IdDocumentType,
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
            .expect("openapi components initialised");
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "OIDC-issued JWT. Same verification posture as the Declaration service.",
                    ))
                    .build(),
            ),
        );
        components.add_security_scheme(
            "devPrincipalHeader",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Recor-Dev-Principal",
                "Dev-only principal shortcut. Refused outside ENVIRONMENT=dev.",
            ))),
        );
    }
}

/// Public entry point: build the OpenAPI document.
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
    (
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
}

#[cfg(test)]
mod tests {
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
    fn covers_every_public_path() {
        let v = doc_json();
        let paths = v["paths"].as_object().expect("paths object present");
        for expected in [
            "/healthz",
            "/readyz",
            "/v1/persons",
            "/v1/persons/{person_id}",
            "/v1/persons/search",
            "/v1/persons/{person_id}/merge-into/{target_id}",
        ] {
            assert!(
                paths.contains_key(expected),
                "missing path {expected} in spec; paths present: {:?}",
                paths.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn register_endpoint_declares_request_and_known_responses() {
        let v = doc_json();
        let op = &v["paths"]["/v1/persons"]["post"];
        assert_eq!(op["operationId"], "registerPerson");
        for status in ["201", "400", "401", "409"] {
            assert!(
                !op["responses"][status].is_null(),
                "expected {status} response on register_person"
            );
        }
    }

    #[test]
    fn key_schemas_declared() {
        let v = doc_json();
        let schemas = v["components"]["schemas"]
            .as_object()
            .expect("components.schemas object");
        for expected in [
            "RegisterPersonRequest",
            "RegisterPersonResponse",
            "GetPersonResponse",
            "SearchPersonsResponse",
            "MergePersonsResponse",
            "PersonAttributes",
            "IdDocument",
            "ErrorEnvelope",
            "ErrorBody",
        ] {
            assert!(
                schemas.contains_key(expected),
                "missing schema {expected}"
            );
        }
    }

    #[test]
    fn security_schemes_are_registered() {
        let v = doc_json();
        let security_schemes = &v["components"]["securitySchemes"];
        for expected in ["bearerAuth", "devPrincipalHeader"] {
            assert!(
                !security_schemes[expected].is_null(),
                "missing security scheme {expected}"
            );
        }
    }
}
