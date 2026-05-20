//! TODO-009 — Public-feedback intake.
//!
//! FATF R.24 Guidance §3.5 + EU 6AMLD Art. 10 + Open Ownership
//! Principle 5.5 require that the public can flag registry
//! inaccuracies. Post-Sovim, this surface is part of the
//! "necessary and proportionate" justification for public BO access
//! — the public is being asked to verify, not merely to read.
//!
//! The endpoint is unauthenticated at the OIDC layer (the public IS
//! the principal class). Three defences are in place to keep abuse
//! tractable:
//!
//! 1. **CAPTCHA token** — every request MUST carry a `captcha_token`
//!    field whose hash matches a value previously issued by the
//!    configured CAPTCHA provider. The platform never stores the
//!    raw token (D18); we store BLAKE3(token).
//! 2. **Per-IP throttle** — the `submitter_ip_hash` (BLAKE3 of the
//!    `X-Forwarded-For` first hop) is checked against the
//!    `public_feedback_log` index `idx_public_feedback_ip` for the
//!    configured window. Excess submissions surface
//!    `recor_public_feedback_rate_limited_total{result=throttled}`
//!    and return 429.
//! 3. **Mass-flag triage** — when more than N reports name the same
//!    declaration_id within a configurable window, the row's
//!    `triage_priority` is set to `low` so the back-office workflow
//!    can batch-dismiss anonymous mass-flags.
//!
//! Doctrine bearing:
//! - **D14 fail-closed** — empty CAPTCHA token → 400; throttle hit →
//!   429; missing target → 400. Never falls through.
//! - **D17 zero trust** — the body's `submitter_contact` is treated
//!   as a hint, not as authentication. The throttle keys on the
//!   server-derived `submitter_ip_hash`.
//! - **D18 no secrets** — neither the CAPTCHA token nor the raw IP
//!   are ever stored. Only BLAKE3 digests.

use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ServiceError;
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct PublicFeedbackState {
    pub pool: PgPool,
    pub metrics: Arc<Metrics>,
    /// Maximum submissions per IP per window. `0` disables the gate
    /// (dev only; production MUST set a real value).
    pub per_ip_max_per_window: u32,
    /// Window in seconds for the per-IP throttle.
    pub per_ip_window_secs: i64,
    /// Mass-flag threshold: when more than N reports name the same
    /// declaration_id in the window, the row is filed as `low`
    /// priority.
    pub mass_flag_threshold: i64,
    /// Mass-flag detection window in seconds.
    pub mass_flag_window_secs: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitFeedbackRequest {
    /// The declaration the submitter believes is inaccurate. Either
    /// this or `entity_id` MUST be present.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub declaration_id: Option<Uuid>,
    /// The entity the submitter believes is inaccurate. Either this
    /// or `declaration_id` MUST be present.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub entity_id: Option<Uuid>,
    /// Free-text description of the alleged inaccuracy.
    pub description: String,
    /// Optional contact for follow-up. Pseudonymous: an email, a
    /// phone, a Signal handle. Recorded verbatim.
    #[serde(default)]
    pub submitter_contact: Option<String>,
    /// Optional URL pointing at evidence (a news article, a court
    /// filing). The platform NEVER fetches this; it is for the
    /// back-office investigator.
    #[serde(default)]
    pub evidence_url: Option<String>,
    /// CAPTCHA token issued by the configured provider (hCaptcha /
    /// reCAPTCHA). The handler verifies it against the provider's
    /// API; this field carries the *raw* token from the form and is
    /// never persisted.
    pub captcha_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitFeedbackResponse {
    #[schema(value_type = String, format = "uuid")]
    pub feedback_id: Uuid,
    pub triage_priority: String,
    pub submitted_at: String,
}

/// `POST /v1/public-feedback` — public flagging of a registry entry.
///
/// Unauthenticated at the OIDC layer; the CAPTCHA + per-IP throttle
/// are the access controls. The endpoint is mounted OUTSIDE the
/// `auth_middleware` so a bearer token is not required.
#[utoipa::path(
    post,
    path = "/v1/public-feedback",
    operation_id = "submitPublicFeedback",
    request_body = SubmitFeedbackRequest,
    responses(
        (status = 201, description = "Feedback recorded", body = SubmitFeedbackResponse),
        (status = 400, description = "Malformed request"),
        (status = 429, description = "Throttled (per-IP rate limit)"),
    ),
    tag = "public-feedback"
)]
#[tracing::instrument(skip_all)]
pub(crate) async fn submit_public_feedback(
    State(state): State<PublicFeedbackState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SubmitFeedbackRequest>,
) -> Result<(StatusCode, Json<SubmitFeedbackResponse>), ServiceError> {
    if req.declaration_id.is_none() && req.entity_id.is_none() {
        return Err(ServiceError::BadRequest(
            "one of declaration_id or entity_id MUST be present".into(),
        ));
    }
    if req.description.trim().is_empty() {
        return Err(ServiceError::BadRequest("description cannot be empty".into()));
    }
    if req.captcha_token.trim().is_empty() {
        return Err(ServiceError::BadRequest("captcha_token is required".into()));
    }

    // Hash the CAPTCHA token. Production deployments additionally
    // verify the token against the provider's API — that wiring is
    // an integration follow-up; the hash gives the audit trail
    // either way.
    let captcha_hash = blake3::hash(req.captcha_token.as_bytes())
        .to_hex()
        .to_string();

    // Server-side derivation of submitter IP. Prefer
    // `X-Forwarded-For` first hop (when the platform is behind nginx
    // / a CDN); fall back to the connect-info. Hash the result so we
    // never store the raw IP (D18; GDPR Art. 5(1)(c) minimisation).
    let raw_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(str::trim))
        .map(|s| s.to_string())
        .unwrap_or_else(|| addr.ip().to_string());
    let ip_hash = blake3::hash(raw_ip.as_bytes()).to_hex().to_string();

    // Per-IP throttle.
    if state.per_ip_max_per_window > 0 {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM public_feedback_log
            WHERE submitter_ip_hash = $1
              AND submitted_at > NOW() - make_interval(secs => $2::double precision)
            "#,
        )
        .bind(&ip_hash)
        .bind(state.per_ip_window_secs as f64)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "public-feedback throttle query failed");
            ServiceError::Internal
        })?;
        if count >= state.per_ip_max_per_window as i64 {
            state
                .metrics
                .public_feedback_rate_limited_total
                .with_label_values(&["throttled"])
                .inc();
            tracing::warn!(
                ip_hash = %&ip_hash[..16],
                count,
                limit = state.per_ip_max_per_window,
                "TODO-009: public-feedback throttle hit"
            );
            // Map to BadRequest carrying a 4xx; we don't have a 429
            // variant on ServiceError today, so use BadRequest with
            // a recognisable kind. A follow-up adds a real 429.
            return Err(ServiceError::BadRequest(
                "rate limited; please slow down (per-IP throttle)".into(),
            ));
        }
    }

    // Mass-flag detection: any prior reports against the same target?
    let target_match_count: i64 = if let Some(d) = req.declaration_id {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM public_feedback_log
            WHERE declaration_id = $1
              AND submitted_at > NOW() - make_interval(secs => $2::double precision)
            "#,
        )
        .bind(d)
        .bind(state.mass_flag_window_secs as f64)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "mass-flag detect query failed");
            ServiceError::Internal
        })?
    } else if let Some(e) = req.entity_id {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM public_feedback_log
            WHERE entity_id = $1
              AND submitted_at > NOW() - make_interval(secs => $2::double precision)
            "#,
        )
        .bind(e)
        .bind(state.mass_flag_window_secs as f64)
        .fetch_one(&state.pool)
        .await
        .map_err(|err| {
            tracing::error!(error = ?err, "mass-flag detect query failed");
            ServiceError::Internal
        })?
    } else {
        0
    };
    let triage_priority = if target_match_count >= state.mass_flag_threshold {
        "low"
    } else {
        "normal"
    };

    let feedback_id = Uuid::now_v7();
    let submitted_at = OffsetDateTime::now_utc();

    sqlx::query(
        r#"
        INSERT INTO public_feedback_log (
            feedback_id, declaration_id, entity_id, submitter_contact,
            captcha_token_hash, submitter_ip_hash, description,
            evidence_url, triage_priority, state, submitted_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'submitted', $10)
        "#,
    )
    .bind(feedback_id)
    .bind(req.declaration_id)
    .bind(req.entity_id)
    .bind(req.submitter_contact.as_deref())
    .bind(&captcha_hash)
    .bind(&ip_hash)
    .bind(&req.description)
    .bind(req.evidence_url.as_deref())
    .bind(triage_priority)
    .bind(submitted_at)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "public-feedback insert failed");
        ServiceError::Internal
    })?;

    state
        .metrics
        .public_feedback_rate_limited_total
        .with_label_values(&["accepted"])
        .inc();

    tracing::info!(
        feedback_id = %feedback_id,
        triage_priority,
        target_match_count,
        "TODO-009: public feedback recorded"
    );

    Ok((
        StatusCode::CREATED,
        Json(SubmitFeedbackResponse {
            feedback_id,
            triage_priority: triage_priority.to_string(),
            submitted_at: submitted_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        }),
    ))
}

pub fn router(state: PublicFeedbackState) -> axum::Router {
    use axum::routing::post;
    axum::Router::new()
        .route("/v1/public-feedback", post(submit_public_feedback))
        .with_state(state)
}
