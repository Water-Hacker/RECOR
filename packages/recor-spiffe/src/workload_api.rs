//! SPIFFE Workload API client surface.
//!
//! The production deployment uses the standardised gRPC Workload API
//! exposed by `spire-agent` over a Unix domain socket
//! (`unix:///tmp/spire-agent/public/api.sock`). The full gRPC client is
//! a follow-up — for the R-LOOP-3 skeleton we expose:
//!
//! 1. A [`WorkloadApi`] trait — the seam between the service and "the
//!    thing that hands us SVIDs". This lets unit tests plug a
//!    wiremock-backed HTTP fixture or a hand-written mock.
//! 2. [`HttpWorkloadApi`] — a thin HTTP-shaped implementation that
//!    fetches a JSON-encoded SVID bundle from a configurable URL.
//!    Useful for:
//!      - local unit tests (with wiremock),
//!      - dev environments running a sidecar that translates the gRPC
//!        Workload API into HTTP (the SPIRE OIDC discovery provider
//!        ships such a sidecar already),
//!      - any agent surface that prefers REST.
//!
//! Production wires `SpiffeClient::new(Arc::new(gRPC client), …)` once
//! the gRPC client lands. The shape that travels between the client
//! and the rest of the crate is [`X509SvidResponse`]; nothing else
//! changes.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::SpiffeError;

/// The payload returned by the Workload API for the calling
/// workload's X.509 SVID. Names match the SPIRE Workload API gRPC
/// surface (1:1 with the `X509SVIDResponse` message), so future
/// migration to a gRPC client is a transport swap not a re-shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X509SvidResponse {
    /// The SPIFFE ID this SVID is bound to.
    pub spiffe_id: String,
    /// PEM-encoded X.509 chain — leaf first, intermediates follow.
    #[serde(with = "pem_bytes")]
    pub chain_pem: Vec<u8>,
    /// PEM-encoded PKCS#8 private key.
    #[serde(with = "pem_bytes")]
    pub key_pem: Vec<u8>,
    /// PEM-encoded trust bundle (concatenated CA roots).
    #[serde(with = "pem_bytes")]
    pub trust_bundle_pem: Vec<u8>,
}

/// Transport-agnostic SVID source.
#[async_trait]
pub trait WorkloadApi: Send + Sync + 'static {
    /// Fetch the X.509 SVID + trust bundle for the calling workload.
    async fn fetch_svid(&self) -> crate::Result<X509SvidResponse>;
}

/// HTTP-backed `WorkloadApi`. Used in unit tests and in dev
/// environments that proxy the gRPC surface through HTTP.
pub struct HttpWorkloadApi {
    base_url: String,
}

impl HttpWorkloadApi {
    /// Construct an HTTP client that fetches the SVID from
    /// `<base_url>/api/v1/svid`. The base URL is whatever the
    /// dev/test fixture exposes.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl WorkloadApi for HttpWorkloadApi {
    async fn fetch_svid(&self) -> crate::Result<X509SvidResponse> {
        // We avoid pulling reqwest into the package's runtime deps
        // (reqwest is already a workspace dep but only the services
        // pull it in) — instead we use a tiny std-only fetch via
        // tokio. For wiremock the request shape is whatever HTTP
        // client we point at it, so any client works.
        //
        // For the skeleton we use a hand-built TCP+HTTP request to
        // keep this crate's compile cost small. Production swaps the
        // whole `HttpWorkloadApi` for the gRPC `WorkloadApi` impl.
        let url = format!("{}/api/v1/svid", self.base_url.trim_end_matches('/'));
        let body = http_get_json(&url)
            .await
            .map_err(|e| SpiffeError::WorkloadApiUnreachable(e.to_string()))?;

        let resp: X509SvidResponse = serde_json::from_slice(&body)
            .map_err(|e| SpiffeError::MalformedSvid(format!("JSON: {e}")))?;
        if resp.chain_pem.is_empty() {
            return Err(SpiffeError::MalformedSvid("empty chain_pem".into()));
        }
        if resp.key_pem.is_empty() {
            return Err(SpiffeError::MalformedSvid("empty key_pem".into()));
        }
        if resp.trust_bundle_pem.is_empty() {
            return Err(SpiffeError::MalformedSvid(
                "empty trust_bundle_pem".into(),
            ));
        }
        if !resp.spiffe_id.starts_with("spiffe://") {
            return Err(SpiffeError::MalformedSvid(format!(
                "spiffe_id missing scheme: {}",
                resp.spiffe_id
            )));
        }
        Ok(resp)
    }
}

/// Tiny HTTP GET — avoids pulling reqwest into the crate so the
/// dependency surface stays small. Uses tokio + raw TCP +
/// `HTTP/1.1`. Sufficient for wiremock-backed tests, NOT for
/// production. Production wires gRPC.
async fn http_get_json(url: &str) -> std::io::Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let (host, port, path) = parse_http_url(url)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad url"))?;
    let mut stream = TcpStream::connect((host.as_str(), port)).await?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;

    let mut buf = Vec::with_capacity(8 * 1024);
    stream.read_to_end(&mut buf).await?;

    // Find the header/body boundary.
    let sep = b"\r\n\r\n";
    let body_start = buf
        .windows(sep.len())
        .position(|w| w == sep)
        .map(|i| i + sep.len())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no body"))?;

    // Naive: assume identity encoding (wiremock defaults to that).
    Ok(buf[body_start..].to_vec())
}

fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match host_port.find(':') {
        Some(i) => (
            host_port[..i].to_string(),
            host_port[i + 1..].parse().ok()?,
        ),
        None => (host_port.to_string(), 80),
    };
    Some((host, port, path.to_string()))
}

/// Serde glue: PEM blobs come over JSON as plain strings; on the
/// wire they're UTF-8 byte slices. We round-trip via `Vec<u8>` so
/// the rest of the crate works on bytes uniformly.
mod pem_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let st = std::str::from_utf8(b).map_err(serde::ser::Error::custom)?;
        s.serialize_str(st)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        Ok(s.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parser_handles_loopback_with_port() {
        let parsed = parse_http_url("http://127.0.0.1:8080/api/v1/svid")
            .expect("parses");
        assert_eq!(parsed.0, "127.0.0.1");
        assert_eq!(parsed.1, 8080);
        assert_eq!(parsed.2, "/api/v1/svid");
    }

    #[test]
    fn url_parser_handles_default_port() {
        let parsed = parse_http_url("http://example.com/x").expect("parses");
        assert_eq!(parsed.1, 80);
        assert_eq!(parsed.2, "/x");
    }

    #[test]
    fn url_parser_rejects_https() {
        // Production gRPC over UDS is the target. The HTTP shim is
        // for wiremock tests; HTTPS isn't part of the contract.
        assert!(parse_http_url("https://example.com").is_none());
    }
}
