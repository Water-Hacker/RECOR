//! HTTP handlers for the verifier.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use crate::auth::{auth_middleware, AuthConfig};
use crate::fabric_client::{FabricClient, FabricClientError};
use crate::projection::ProjectionRepo;
use crate::report::{build_report, VerificationReport};

#[derive(Clone)]
pub struct AppState {
    pub fabric: Arc<dyn FabricClient>,
    pub projection: Arc<dyn ProjectionRepo>,
}

/// Construct the public router. The verify endpoint is gated by the
/// OIDC auth middleware (FIND-001). Probes are intentionally
/// unauthenticated — they neither read nor return PII.
pub fn router(state: AppState, auth: AuthConfig) -> Router {
    let protected = Router::new()
        .route("/v1/audit/verify/{declaration_id}", get(verify))
        .route_layer(axum::middleware::from_fn_with_state(
            auth,
            auth_middleware,
        ))
        .with_state(state.clone());

    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state);

    protected.merge(public)
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz() -> impl IntoResponse {
    (StatusCode::OK, "ready")
}

#[instrument(skip(state))]
async fn verify(
    State(state): State<AppState>,
    Path(declaration_id): Path<String>,
) -> impl IntoResponse {
    let decl_uuid = match Uuid::parse_str(&declaration_id) {
        Ok(u) => u,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "declaration_id must be a UUID").into_response()
        }
    };

    // Step 1: query Fabric. Fail-closed: if Fabric is unreachable we
    // return 503 rather than degrading to "projection-only" verification
    // (which would defeat the entire purpose of anchoring).
    let on_chain = match state.fabric.list_for_declaration(&decl_uuid.to_string()).await {
        Ok(v) => v,
        Err(FabricClientError::Transport(msg)) | Err(FabricClientError::Decode(msg)) => {
            warn!(error = %msg, "fabric query failed");
            return (StatusCode::SERVICE_UNAVAILABLE, "fabric unreachable").into_response();
        }
        Err(FabricClientError::Upstream(msg)) => {
            error!(error = %msg, "fabric upstream error");
            return (StatusCode::BAD_GATEWAY, "fabric upstream error").into_response();
        }
    };

    // Step 2: fetch each event from the projection. Failures here are
    // tolerated — the report distinguishes "we couldn't read the
    // projection" from "the projection didn't have this event". The
    // chosen design reads the projection per on-chain entry; a future
    // optimisation can fetch the whole declaration in one query.
    let mut projection_rows = Vec::with_capacity(on_chain.len());
    for entry in &on_chain {
        let event_uuid = match Uuid::parse_str(&entry.event_id) {
            Ok(u) => u,
            Err(_) => {
                warn!(event_id = %entry.event_id, "on-chain entry has malformed event_id");
                continue;
            }
        };
        match state.projection.fetch_event_by_event_id(event_uuid).await {
            Ok(Some(row)) => projection_rows.push(row),
            Ok(None) => {} // build_report treats this as MissingProjection
            Err(e) => {
                warn!(error = ?e, event_id = %entry.event_id, "projection fetch failed");
            }
        }
    }

    let report: VerificationReport = build_report(decl_uuid, on_chain, projection_rows);
    info!(declaration_id = %decl_uuid, result = ?report.result, "verification complete");
    (StatusCode::OK, Json(report)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthConfig;
    use crate::fabric_client::{InMemoryFabricClient, OnChainEntry};
    use crate::hashing::derive_receipt_hash;
    use crate::projection::{InMemoryProjectionRepo, ProjectionRow};
    use axum::body::Body;
    use http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    /// Dev-mode auth — every test request must carry
    /// `X-Recor-Dev-Principal` to pass the middleware.
    fn test_auth() -> AuthConfig {
        AuthConfig {
            is_dev: true,
            oidc: None,
        }
    }

    fn req(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("x-recor-dev-principal", "spiffe://recor.cm/test")
            .body(Body::empty())
            .unwrap()
    }

    fn payload(decl: Uuid, ts: &str) -> serde_json::Value {
        json!({
            "declaration_id": decl.to_string(),
            "submitted_at": ts,
            "data": {"a": 1},
        })
    }

    fn proj_row(eid: Uuid, decl: Uuid, ts: &str) -> ProjectionRow {
        let p = payload(decl, ts);
        let mut clone = p.clone();
        clone.as_object_mut().unwrap().remove("receipt_hash_hex");
        let receipt = derive_receipt_hash(&clone);
        ProjectionRow {
            event_id: eid,
            declaration_id: decl,
            event_type: "declaration.submitted.v1".to_string(),
            event_payload: p,
            receipt_hash_hex: receipt,
            ts: ts.to_string(),
        }
    }

    #[tokio::test]
    async fn returns_authentic_for_matching_chain_and_projection() {
        let decl = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let row = proj_row(eid, decl, "2026-05-12T10:00:00Z");
        let projection = Arc::new(InMemoryProjectionRepo::new());
        projection.add(row.clone()).await;
        let fabric = Arc::new(InMemoryFabricClient::new());
        fabric
            .add(
                &decl.to_string(),
                OnChainEntry {
                    event_id: eid.to_string(),
                    declaration_id: decl.to_string(),
                    receipt_hash_hex: row.receipt_hash_hex.clone(),
                    ts: row.ts.clone(),
                    tx_id: "tx-1".to_string(),
                },
            )
            .await;

        let app = router(
            AppState {
                fabric,
                projection,
            },
            test_auth(),
        );
        let resp = app
            .oneshot(req(&format!("/v1/audit/verify/{decl}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["result"], "authentic");
    }

    #[tokio::test]
    async fn returns_503_when_fabric_unreachable() {
        let decl = Uuid::new_v4();
        let fabric = Arc::new(InMemoryFabricClient::new());
        fabric.set_fail(true).await;
        let projection = Arc::new(InMemoryProjectionRepo::new());

        let app = router(
            AppState {
                fabric,
                projection,
            },
            test_auth(),
        );
        let resp = app
            .oneshot(req(&format!("/v1/audit/verify/{decl}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn returns_400_on_malformed_declaration_id() {
        let app = router(
            AppState {
                fabric: Arc::new(InMemoryFabricClient::new()),
                projection: Arc::new(InMemoryProjectionRepo::new()),
            },
            test_auth(),
        );
        let resp = app
            .oneshot(req("/v1/audit/verify/not-a-uuid"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn detects_hash_mismatch_as_tampered() {
        let decl = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let row = proj_row(eid, decl, "2026-05-12T10:00:00Z");
        let projection = Arc::new(InMemoryProjectionRepo::new());
        projection.add(row.clone()).await;
        let fabric = Arc::new(InMemoryFabricClient::new());
        fabric
            .add(
                &decl.to_string(),
                OnChainEntry {
                    event_id: eid.to_string(),
                    declaration_id: decl.to_string(),
                    receipt_hash_hex: "ff".repeat(32), // wrong
                    ts: row.ts.clone(),
                    tx_id: "tx-1".to_string(),
                },
            )
            .await;

        let app = router(
            AppState {
                fabric,
                projection,
            },
            test_auth(),
        );
        let resp = app
            .oneshot(req(&format!("/v1/audit/verify/{decl}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["result"], "tampered");
    }

    /// FIND-001 (audit Sprint 0). The verify endpoint must refuse
    /// any caller that does not carry a verified principal.
    #[tokio::test]
    async fn unauthenticated_call_is_refused_401() {
        let decl = Uuid::new_v4();
        let app = router(
            AppState {
                fabric: Arc::new(InMemoryFabricClient::new()),
                projection: Arc::new(InMemoryProjectionRepo::new()),
            },
            test_auth(),
        );
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/audit/verify/{decl}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Probes are intentionally unauthenticated — they neither read
    /// nor return PII.
    #[tokio::test]
    async fn probes_are_unauthenticated() {
        let app = router(
            AppState {
                fabric: Arc::new(InMemoryFabricClient::new()),
                projection: Arc::new(InMemoryProjectionRepo::new()),
            },
            test_auth(),
        );
        let healthz = app
            .clone()
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(healthz.status(), StatusCode::OK);
        let readyz = app
            .oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(readyz.status(), StatusCode::OK);
    }
}
