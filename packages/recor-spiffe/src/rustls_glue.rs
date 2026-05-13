//! rustls 0.23 glue.
//!
//! Two builders:
//!
//! - [`build_server_config`] — inbound mTLS. The local SVID is the
//!   server certificate; peer certificates are required and verified
//!   against the trust bundle.
//! - [`build_client_config`] — outbound mTLS. The local SVID is the
//!   client certificate; the server certificate is verified against
//!   the trust bundle.
//!
//! Both use the `ring` cryptographic provider (`rustls`'s default
//! when the `ring` feature is enabled — see `Cargo.toml`). Provider
//! selection is locked at workspace level so all RÉCOR services
//! produce identical TLS behaviour.
//!
//! Peer-identity extraction is done in [`peer_spiffe_id_from_cert`]:
//! we parse the X.509 leaf with `x509-parser` and pull the **URI**
//! Subject Alternative Name. SPIFFE-spec compliant SVIDs always
//! carry exactly one URI SAN of the form
//! `spiffe://<trust_domain>/<path>`; if the cert has multiple URI
//! SANs we take the first one (and the workload-api fixture builds
//! single-SAN certs, matching SPIRE production behaviour).

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

use crate::{Result, SpiffeError, SvidBundle};

/// Parse a PEM-formatted certificate chain into a sequence of
/// rustls-typed [`CertificateDer`].
pub fn pem_chain_to_der(pem_bytes: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    let mut cursor = std::io::Cursor::new(pem_bytes);
    let mut out = Vec::new();
    for cert in rustls_pemfile::certs(&mut cursor) {
        let cert = cert.map_err(|e| {
            SpiffeError::MalformedSvid(format!("pem chain parse: {e}"))
        })?;
        out.push(cert);
    }
    if out.is_empty() {
        return Err(SpiffeError::MalformedSvid(
            "chain pem contained zero certificates".into(),
        ));
    }
    Ok(out)
}

/// Parse a PEM-formatted PKCS#8 private key into a rustls-typed
/// [`PrivateKeyDer`].
pub fn pem_key_to_der(pem_bytes: &[u8]) -> Result<PrivateKeyDer<'static>> {
    let mut cursor = std::io::Cursor::new(pem_bytes);
    // SPIRE issues PKCS#8 keys; we accept SEC1-format too for
    // compatibility with hand-rolled fixtures.
    if let Some(key) = rustls_pemfile::pkcs8_private_keys(&mut cursor)
        .next()
        .transpose()
        .map_err(|e| SpiffeError::MalformedSvid(format!("pkcs8 key parse: {e}")))?
    {
        return Ok(PrivateKeyDer::Pkcs8(key));
    }
    let mut cursor = std::io::Cursor::new(pem_bytes);
    if let Some(key) = rustls_pemfile::ec_private_keys(&mut cursor)
        .next()
        .transpose()
        .map_err(|e| SpiffeError::MalformedSvid(format!("sec1 key parse: {e}")))?
    {
        return Ok(PrivateKeyDer::Sec1(key));
    }
    Err(SpiffeError::MalformedSvid(
        "key pem did not contain a PKCS#8 or SEC1 private key".into(),
    ))
}

/// Build a `rustls::ServerConfig` for inbound mTLS using the local
/// SVID + trust bundle.
pub fn build_server_config(bundle: &SvidBundle) -> Result<Arc<ServerConfig>> {
    let chain = bundle.chain_der()?;
    let key = bundle.key_der()?;
    let roots = roots_from_bundle(bundle)?;

    // Require + verify a client certificate. This is the D17
    // zero-trust gate at the TLS layer; the middleware then maps the
    // verified identity to the application-level allowlist.
    let verifier =
        rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| {
                SpiffeError::RustlsConfig(format!("client verifier build: {e}"))
            })?;

    let cfg = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(chain, key)
        .map_err(|e| SpiffeError::RustlsConfig(format!("server config: {e}")))?;
    Ok(Arc::new(cfg))
}

/// Build a `rustls::ClientConfig` for outbound mTLS using the local
/// SVID + trust bundle.
pub fn build_client_config(bundle: &SvidBundle) -> Result<Arc<ClientConfig>> {
    let chain = bundle.chain_der()?;
    let key = bundle.key_der()?;
    let roots = roots_from_bundle(bundle)?;

    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(chain, key)
        .map_err(|e| SpiffeError::RustlsConfig(format!("client config: {e}")))?;
    Ok(Arc::new(cfg))
}

fn roots_from_bundle(bundle: &SvidBundle) -> Result<RootCertStore> {
    let der = bundle.trust_bundle_der()?;
    let mut roots = RootCertStore::empty();
    let mut accepted = 0usize;
    for cert in der {
        match roots.add(cert) {
            Ok(()) => accepted += 1,
            Err(e) => {
                return Err(SpiffeError::MalformedSvid(format!(
                    "trust bundle reject: {e}"
                )))
            }
        }
    }
    if accepted == 0 {
        return Err(SpiffeError::MalformedSvid(
            "trust bundle had zero accepted roots".into(),
        ));
    }
    Ok(roots)
}

/// Pull the SPIFFE ID out of a peer certificate's URI SAN.
///
/// Returns `Err(SpiffeError::PeerHasNoSpiffeId(_))` for certificates
/// that carry no URI SAN, malformed SANs, or URIs that don't start
/// with `spiffe://`.
pub fn peer_spiffe_id_from_cert(cert: &CertificateDer<'_>) -> Result<crate::SpiffeId> {
    use x509_parser::extensions::{GeneralName, ParsedExtension};
    use x509_parser::prelude::FromDer;

    let (_, parsed) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
        .map_err(|e| SpiffeError::PeerHasNoSpiffeId(format!("x509 parse: {e}")))?;
    for ext in parsed.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::URI(uri) = name {
                    if uri.starts_with("spiffe://") {
                        return crate::SpiffeId::parse(*uri);
                    }
                }
            }
        }
    }
    Err(SpiffeError::PeerHasNoSpiffeId(
        "no URI SAN on peer certificate".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-crafted PEM chain that parses but is intentionally
    /// shaped to fail `roots.add` when fed as a CA root. Used by the
    /// "garbage trust bundle → error" path test.
    const NOT_REALLY_A_CERT: &[u8] = b"-----BEGIN CERTIFICATE-----\nQUFBQQ==\n-----END CERTIFICATE-----\n";

    #[test]
    fn pem_chain_to_der_rejects_empty_input() {
        let r = pem_chain_to_der(b"");
        assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
    }

    #[test]
    fn pem_chain_to_der_surfaces_zero_certs() {
        let r = pem_chain_to_der(b"# nothing here\n");
        assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
    }

    #[test]
    fn pem_key_to_der_rejects_garbage() {
        let r = pem_key_to_der(b"-----BEGIN UNKNOWN-----\nQUE=\n-----END UNKNOWN-----\n");
        assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
    }

    #[test]
    fn roots_from_bundle_rejects_malformed_pem() {
        let bundle = SvidBundle {
            chain_pem: b"".to_vec(),
            key_pem: b"".to_vec(),
            trust_bundle_pem: NOT_REALLY_A_CERT.to_vec(),
            spiffe_id: "spiffe://recor.cm/x".into(),
        };
        // Either the pem parser refuses, or rustls's `roots.add` refuses.
        // Both surface as `SpiffeError::MalformedSvid`.
        let r = roots_from_bundle(&bundle);
        assert!(matches!(r, Err(SpiffeError::MalformedSvid(_))));
    }
}
