//! Command handlers — one function per subcommand. Each handler is
//! a thin shim over `reqwest` that turns the HTTP response into an
//! operator-facing string (the binary prints it as-is).
//!
//! The shape pattern: every handler accepts `(&CliConfig, &reqwest::
//! Client, …command-specific args)` and returns `Result<String>`. The
//! string is the human-readable output; errors propagate up to the
//! main binary which prints them on stderr and exits non-zero.

use anyhow::{anyhow, Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::{auth_headers, build_url, CliConfig, HealthResponse, Service};

/// `recor-cli health <service>` — issues `GET <base>/<service>/healthz`.
/// Anonymous; never sends an Authorization header even when one is set
/// (probes are not authentication-scoped).
pub async fn health(cfg: &CliConfig, http: &reqwest::Client, svc: Service) -> Result<String> {
    let url = build_url(cfg, svc, "/healthz")?;
    let headers = auth_headers(cfg, false)?;
    let resp = http
        .get(&url)
        .headers(headers)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body = resp.text().await.context("read response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "{svc} returned HTTP {status}\nbody: {body}",
            svc = svc.display()
        ));
    }
    // Parse permissively; if the response was a different shape we
    // still print the raw body so the operator can see what came back.
    match serde_json::from_str::<HealthResponse>(&body) {
        Ok(h) => Ok(format!(
            "{svc} OK (status={status})\n  reported: {report}",
            svc = svc.display(),
            status = h.status,
            report = serde_json::to_string_pretty(&h.extras).unwrap_or_default()
        )),
        Err(_) => Ok(format!(
            "{svc} responded {status} but body was not the expected JSON shape:\n{body}",
            svc = svc.display()
        )),
    }
}

/// `recor-cli verify <declaration-id>` — calls the audit verifier and
/// prints the structured report.
pub async fn verify(
    cfg: &CliConfig,
    http: &reqwest::Client,
    declaration_id: &str,
) -> Result<String> {
    let path = format!("/v1/audit/verify/{declaration_id}");
    let url = build_url(cfg, Service::AuditVerifier, &path)?;
    // The verifier endpoint accepts the operator's token if one is
    // set (it's principal-gated) but it is NOT mandatory for the CLI
    // path — anonymous calls land at the dev gate or get rejected by
    // the server.
    let headers = auth_headers(cfg, false)?;
    let resp = http
        .get(&url)
        .headers(headers)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body = resp.text().await.context("read response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "audit-verifier returned HTTP {status} for declaration {declaration_id}\nbody: {body}"
        ));
    }
    // Pretty-print whatever JSON came back (the verifier's report
    // shape lives in `apps/audit-verifier/src/report.rs`; we don't
    // re-declare it here so the CLI stays decoupled from the
    // verifier's internal types).
    let parsed: serde_json::Value =
        serde_json::from_str(&body).with_context(|| format!("parse JSON body: {body}"))?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or(body))
}

/// `recor-cli sanctions search <name>` — calls the v-engine sanctions
/// search adapter. Admin token required; the underlying endpoint
/// requires an admin principal (D17 zero trust).
///
/// The wire endpoint is `POST /v1/internal/sanctions/search` with
/// `{"name": "..."}`. The CLI sends the request and prints the JSON
/// response verbatim (pretty-printed); the v-engine controls the
/// response shape.
pub async fn sanctions_search(
    cfg: &CliConfig,
    http: &reqwest::Client,
    name: &str,
) -> Result<String> {
    let url = build_url(
        cfg,
        Service::VerificationEngine,
        "/v1/internal/sanctions/search",
    )?;
    let headers = auth_headers(cfg, true)?; // admin-only — D14 fail-closed
    let resp = http
        .post(&url)
        .headers(headers)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let body = resp.text().await.context("read response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "sanctions search returned HTTP {status} for name '{name}'\nbody: {body}"
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .with_context(|| format!("parse JSON body from sanctions search: {body}"))?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or(body))
}

/// `recor-cli admin dlq list <service>` — lists dead-lettered rows.
///
/// The route shape differs per service to avoid ambiguity at the
/// operator's terminal (declaration's `/v1/internal/outbox-dlq` vs.
/// v-engine's `/v1/internal/verification-outbox-dlq`); the CLI maps
/// each known service to its DLQ path.
pub async fn dlq_list(cfg: &CliConfig, http: &reqwest::Client, svc: Service) -> Result<String> {
    let path = dlq_path(svc, None)?;
    let url = build_url(cfg, svc, &path)?;
    let headers = auth_headers(cfg, true)?; // admin-only
    let resp = http
        .get(&url)
        .headers(headers)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body = resp.text().await.context("read response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "{svc} DLQ list returned HTTP {status}\nbody: {body}",
            svc = svc.display()
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .with_context(|| format!("parse JSON body from DLQ list: {body}"))?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or(body))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DlqReplayResponse {
    /// Server-side confirmation of the row that was moved back. Shape
    /// is service-defined; we capture as a free-form value.
    #[serde(flatten)]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

/// `recor-cli admin dlq replay <service> <id>` — atomically moves a
/// dead-lettered row back onto the outbox.
pub async fn dlq_replay(
    cfg: &CliConfig,
    http: &reqwest::Client,
    svc: Service,
    id: &str,
) -> Result<String> {
    let path = dlq_path(svc, Some(id))?;
    let url = build_url(cfg, svc, &path)?;
    let headers = auth_headers(cfg, true)?; // admin-only
    let resp = http
        .post(&url)
        .headers(headers)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let body = resp.text().await.context("read response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "{svc} DLQ replay {id} returned HTTP {status}\nbody: {body}",
            svc = svc.display()
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .with_context(|| format!("parse JSON body from DLQ replay: {body}"))?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or(body))
}

/// Map service → its service-specific DLQ path. When `id` is supplied,
/// returns the per-row replay path; otherwise the list path.
///
/// Only the two services that own a DLQ surface (declaration +
/// v-engine) are valid here; the others error rather than emit a
/// 404-bound URL.
fn dlq_path(svc: Service, id: Option<&str>) -> Result<String> {
    let base = match svc {
        Service::Declaration => "/v1/internal/outbox-dlq",
        Service::VerificationEngine => "/v1/internal/verification-outbox-dlq",
        Service::Person | Service::Entity | Service::AuditVerifier => {
            return Err(anyhow!(
                "{svc} does not expose a DLQ admin surface; only declaration + verification-engine do",
                svc = svc.display()
            ));
        }
    };
    Ok(match id {
        None => base.to_string(),
        Some(id) => format!("{base}/{id}/replay"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlq_path_declaration_list() {
        assert_eq!(
            dlq_path(Service::Declaration, None).unwrap(),
            "/v1/internal/outbox-dlq"
        );
    }

    #[test]
    fn dlq_path_verification_engine_replay() {
        assert_eq!(
            dlq_path(Service::VerificationEngine, Some("abc-123")).unwrap(),
            "/v1/internal/verification-outbox-dlq/abc-123/replay"
        );
    }

    #[test]
    fn dlq_path_refuses_non_dlq_services() {
        for svc in [Service::Person, Service::Entity, Service::AuditVerifier] {
            let err = dlq_path(svc, None).unwrap_err();
            assert!(format!("{err}").contains("does not expose a DLQ"));
        }
    }
}
