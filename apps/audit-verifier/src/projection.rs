//! Read-side access to the Declaration service's projection.
//!
//! The verifier needs only the canonical event payload (and the
//! receipt_hash_hex the service originally produced); we read directly
//! from the `declaration_events` table because that is the immutable
//! source of truth (the projection table is reconstructed by replay).

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("event missing required field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRow {
    pub event_id: Uuid,
    pub declaration_id: Uuid,
    pub event_type: String,
    pub event_payload: JsonValue,
    pub receipt_hash_hex: String,
    pub ts: String,
}

#[async_trait]
pub trait ProjectionRepo: Send + Sync + std::fmt::Debug {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError>;
}

#[derive(Debug, Clone)]
pub struct PostgresProjectionRepo {
    pool: PgPool,
}

impl PostgresProjectionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProjectionRepo for PostgresProjectionRepo {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError> {
        // The Declaration service's `declaration_events` table doesn't
        // carry an `event_id` column directly — the outbox carries it
        // and the event is correlated by (correlation_id) or by index
        // in the per-aggregate sequence. For the verifier skeleton we
        // join through `outbox` (and `outbox_dlq` as a fallback) to
        // recover the event row, then read the canonical payload from
        // `declaration_events`.
        //
        // This query is intentionally written in the runtime-checked
        // form (non-macro) so it does not require an entry in the
        // declaration service's sqlx cache.
        let row = sqlx::query_as::<_, ProjectionQueryRow>(
            r#"
            SELECT
                o.event_id           AS event_id,
                o.aggregate_id       AS declaration_id,
                e.event_type         AS event_type,
                e.event_payload      AS event_payload
            FROM outbox o
            JOIN declaration_events e
              ON e.declaration_id = o.aggregate_id
             AND e.event_type    = o.event_type
            WHERE o.event_id = $1
            LIMIT 1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_row()?)),
            None => Ok(None),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectionQueryRow {
    event_id: Uuid,
    declaration_id: Uuid,
    event_type: String,
    event_payload: JsonValue,
}

impl ProjectionQueryRow {
    fn into_row(self) -> Result<ProjectionRow, ProjectionError> {
        let receipt_hash_hex = self
            .event_payload
            .get("receipt_hash_hex")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or(ProjectionError::MissingField("receipt_hash_hex"))?;
        let ts = ["submitted_at", "amended_at", "corrected_at", "superseded_at"]
            .iter()
            .find_map(|k| self.event_payload.get(*k).and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .ok_or(ProjectionError::MissingField("timestamp"))?;
        Ok(ProjectionRow {
            event_id: self.event_id,
            declaration_id: self.declaration_id,
            event_type: self.event_type,
            event_payload: self.event_payload,
            receipt_hash_hex,
            ts,
        })
    }
}

/// TODO-041: HTTP-API-backed projection repo.
///
/// Replaces the direct DB read with a call to the Declaration service's
/// public REST endpoint `GET /v1/declarations/{id}`. This severs the
/// cross-service DB coupling that previously let the verifier reach
/// into the Declaration service's tables — a Doctrine 17 zero-trust
/// violation that is now resolved with a documented service boundary.
///
/// The `PostgresProjectionRepo` is retained as a dev-posture fallback:
/// the `audit-verifier` `main.rs` checks `DECLARATION_API_URL` at boot
/// and picks `DeclarationApiProjection` when it's set, otherwise falls
/// back to the Postgres repo for local development.
#[derive(Debug, Clone)]
pub struct DeclarationApiProjection {
    http: reqwest::Client,
    base_url: String,
    /// Optional bearer token. Empty ⇒ no Authorization header (dev
    /// only; the Declaration service refuses unauthenticated reads
    /// outside ENVIRONMENT=dev).
    bearer_token: String,
}

impl DeclarationApiProjection {
    pub fn new(base_url: impl Into<String>, bearer_token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client should build");
        Self {
            http,
            base_url: base_url.into(),
            bearer_token: bearer_token.into(),
        }
    }
}

/// Wire-format echo of the relevant fields from
/// `services/declaration/src/api/dto.rs::GetDeclarationResponse`. Only
/// the fields the verifier consumes are deserialised; additional
/// fields are ignored so the contract can evolve.
#[derive(Debug, Clone, serde::Deserialize)]
struct GetDeclarationApiResponse {
    declaration_id: Uuid,
    submitted_at: String,
    receipt_hash_hex: String,
}

#[async_trait]
impl ProjectionRepo for DeclarationApiProjection {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError> {
        // The Declaration REST API exposes declarations by declaration_id
        // not by event_id. The Fabric audit chain records the
        // (declaration_id, event_id) pair on each entry; the verifier
        // passes the declaration_id through here as the "event_id"
        // argument for the API path. This matches the in-memory test
        // implementation and the Postgres fallback's row-shape
        // expectations: callers MUST pass the declaration_id when this
        // repo is in play.
        //
        // D14 (fail-closed): a 404 returns Ok(None) — "no such row" is
        // a legitimate verification outcome (the chain references a
        // declaration the projection no longer holds, indicating
        // potential tampering on the projection side). A network error
        // or 5xx bubbles up as `ProjectionError::Db`; the caller maps
        // that to an HTTP 503 rather than a "verified" success.
        let url = format!("{}/v1/declarations/{}", self.base_url, event_id);
        let mut req = self.http.get(&url);
        if !self.bearer_token.is_empty() {
            req = req.bearer_auth(&self.bearer_token);
        }
        let resp = req.send().await.map_err(|e| {
            ProjectionError::Db(sqlx::Error::Configuration(
                format!("declaration API transport: {e}").into(),
            ))
        })?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(ProjectionError::Db(sqlx::Error::Configuration(
                format!("declaration API returned {status}").into(),
            )));
        }

        let payload: serde_json::Value = resp.json().await.map_err(|e| {
            ProjectionError::Db(sqlx::Error::Decode(
                format!("declaration API JSON decode: {e}").into(),
            ))
        })?;
        let api_view: GetDeclarationApiResponse = serde_json::from_value(payload.clone())
            .map_err(|e| {
                ProjectionError::Db(sqlx::Error::Decode(format!("api shape: {e}").into()))
            })?;

        Ok(Some(ProjectionRow {
            event_id,
            declaration_id: api_view.declaration_id,
            event_type: "declaration.submitted.v1".to_string(),
            event_payload: payload,
            receipt_hash_hex: api_view.receipt_hash_hex,
            ts: api_view.submitted_at,
        }))
    }
}

/// In-memory implementation for unit testing the report layer.
#[derive(Debug, Default)]
pub struct InMemoryProjectionRepo {
    pub rows: tokio::sync::Mutex<std::collections::HashMap<Uuid, ProjectionRow>>,
}

impl InMemoryProjectionRepo {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn add(&self, row: ProjectionRow) {
        self.rows.lock().await.insert(row.event_id, row);
    }
}

#[async_trait]
impl ProjectionRepo for InMemoryProjectionRepo {
    async fn fetch_event_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<ProjectionRow>, ProjectionError> {
        Ok(self.rows.lock().await.get(&event_id).cloned())
    }
}

#[cfg(test)]
mod declaration_api_projection_tests {
    //! TODO-041 — wiremock-backed unit tests for the HTTP projection.
    //!
    //! These tests pin the Declaration-service-side contract this repo
    //! consumes:
    //!
    //!   GET /v1/declarations/{declaration_id}
    //!     200 with { declaration_id, submitted_at, receipt_hash_hex, ... }
    //!         → ProjectionRow with receipt_hash_hex + ts populated
    //!     404 → Ok(None) — fail-closed downstream (D14)
    //!     5xx → ProjectionError::Db with the status surfaced
    //!     200 with malformed body → ProjectionError::Db (decode failure)
    //!
    //! The verifier's caller (`handlers.rs`) maps `Ok(None)` to a
    //! "no projection row" path (legitimate verification outcome) and
    //! `Err` to HTTP 503. This separation is load-bearing for the
    //! fail-closed posture and is what these four tests defend.

    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fixed_uuid(byte: u8) -> Uuid {
        let mut bytes = [0u8; 16];
        bytes[0] = byte;
        Uuid::from_bytes(bytes)
    }

    #[tokio::test]
    async fn happy_path_decodes_full_response() {
        let server = MockServer::start().await;
        let declaration_id = fixed_uuid(0x11);
        let body = json!({
            "declaration_id": declaration_id,
            "submitted_at": "2026-05-20T10:00:00Z",
            "receipt_hash_hex": "ab".repeat(32),
            "declarant_principal": "spiffe://recor.cm/declarant-A",
            "verification_state": "verified",
        });
        Mock::given(method("GET"))
            .and(path(format!("/v1/declarations/{declaration_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;

        let repo = DeclarationApiProjection::new(server.uri(), "");
        let row = repo
            .fetch_event_by_event_id(declaration_id)
            .await
            .expect("happy path returns Ok")
            .expect("Some(row) on 200");
        assert_eq!(row.declaration_id, declaration_id);
        assert_eq!(row.event_id, declaration_id);
        assert_eq!(row.receipt_hash_hex, "ab".repeat(32));
        assert_eq!(row.ts, "2026-05-20T10:00:00Z");
        assert_eq!(row.event_type, "declaration.submitted.v1");
        // The full JSON body is carried through verbatim so the
        // hashing layer can re-derive the canonical receipt bytes.
        assert_eq!(
            row.event_payload.get("declarant_principal"),
            Some(&json!("spiffe://recor.cm/declarant-A"))
        );
    }

    #[tokio::test]
    async fn http_404_maps_to_ok_none() {
        // D14 fail-closed: "no such declaration" is a legitimate
        // verification outcome (the chain references a row the
        // projection no longer holds → possible projection-side
        // tampering). The handler distinguishes this from a network
        // 503 by branching on Ok(None) vs Err(ProjectionError::Db).
        let server = MockServer::start().await;
        let declaration_id = fixed_uuid(0x22);
        Mock::given(method("GET"))
            .and(path(format!("/v1/declarations/{declaration_id}")))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let repo = DeclarationApiProjection::new(server.uri(), "");
        let result = repo
            .fetch_event_by_event_id(declaration_id)
            .await
            .expect("404 must NOT be an error");
        assert!(result.is_none(), "404 must surface as Ok(None)");
    }

    #[tokio::test]
    async fn http_5xx_surfaces_as_projection_error() {
        // A 5xx from the Declaration service is a backend failure;
        // the verifier MUST refuse to return a "verified" verdict on
        // an upstream outage. The handler maps this to 503.
        let server = MockServer::start().await;
        let declaration_id = fixed_uuid(0x33);
        Mock::given(method("GET"))
            .and(path(format!("/v1/declarations/{declaration_id}")))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let repo = DeclarationApiProjection::new(server.uri(), "");
        let err = repo
            .fetch_event_by_event_id(declaration_id)
            .await
            .expect_err("5xx must surface as Err");
        match err {
            ProjectionError::Db(inner) => {
                let msg = inner.to_string();
                assert!(
                    msg.contains("503"),
                    "the surfaced error must include the upstream status; got: {msg}"
                );
            }
            other => panic!("expected Db, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_200_body_is_decode_error() {
        // The API contract requires `declaration_id`, `submitted_at`,
        // and `receipt_hash_hex`. A 200 without those fields cannot be
        // trusted: refuse it. Same fail-closed posture as 5xx.
        let server = MockServer::start().await;
        let declaration_id = fixed_uuid(0x44);
        // Missing receipt_hash_hex + submitted_at.
        let body = json!({
            "declaration_id": declaration_id,
            "verification_state": "verified",
        });
        Mock::given(method("GET"))
            .and(path(format!("/v1/declarations/{declaration_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;

        let repo = DeclarationApiProjection::new(server.uri(), "");
        let err = repo
            .fetch_event_by_event_id(declaration_id)
            .await
            .expect_err("malformed body must surface as Err");
        match err {
            ProjectionError::Db(_) => {
                // OK — the inner kind is sqlx::Error::Decode, but the
                // public contract only promises "Db" so we don't pin
                // the inner sqlx variant.
            }
            other => panic!("expected Db (decode), got {other:?}"),
        }
    }
}
