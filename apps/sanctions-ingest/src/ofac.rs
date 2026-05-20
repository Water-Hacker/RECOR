//! TODO-014 — OFAC SDN feed parser.
//!
//! Parses the OFAC SDN XML feed into a list of [`SdnEntry`] records
//! suitable for upserting into the verification engine's
//! `sanctions_persons` table. The XML schema is documented at
//! <https://www.treasury.gov/ofac/downloads/sdn.xml>; this parser
//! handles the v1 schema (current as of 2026-Q2).
//!
//! Network fetching is the responsibility of the binary wrapper;
//! this library accepts an XML byte slice so the same parser is
//! exercised in tests against pinned fixtures.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdnParseError {
    #[error("XML parse failure: {0}")]
    Xml(#[from] SerdeXmlRsMinFailure),
    #[error("empty SDN feed (no entries)")]
    Empty,
}

// Placeholder error type. The real serde_xml_rs crate's error type
// would live here; we stub it so the parser shape is in place for
// the operator's first cut. The full XML wiring (an `xml-rs` or
// `quick-xml` dependency + the model below) lands in the
// TODO-014-OFAC follow-up.
#[derive(Debug, thiserror::Error)]
#[error("xml: {0}")]
pub struct SerdeXmlRsMinFailure(pub String);

#[derive(Debug, Clone, Deserialize)]
pub struct SdnEntry {
    /// Stable OFAC identifier. Becomes `source_id` in the
    /// `sanctions_persons` table.
    pub uid: String,
    /// Primary name as published.
    pub primary_name: String,
    /// Aliases / aka entries.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// OFAC list type (`SDN` | `NS-PLC` | etc.). The platform's
    /// `source` column is fixed to `ofac_sdn` for now; the list_type
    /// is captured as an `aliases` element on the row.
    pub list_type: String,
    /// Designation as published by OFAC.
    pub designation: String,
}

/// Parse the OFAC SDN XML. This is intentionally a skeleton — the
/// canonical XML schema requires a concrete `quick-xml` / `xml-rs`
/// dependency that is added in the TODO-014-OFAC follow-up. Today,
/// the function accepts the raw bytes and returns an error so the
/// binary wrapper can exercise the surrounding flow (digest, sanity
/// check, log) without needing the full XML model.
pub fn parse_sdn(bytes: &[u8]) -> Result<Vec<SdnEntry>, SdnParseError> {
    if bytes.is_empty() {
        return Err(SdnParseError::Empty);
    }
    // TODO-014-OFAC: replace with `quick-xml` deserialise once the
    // dep lands. The operator wires `cargo add quick-xml --features
    // serialize` + populates the schema; the call sites + tests are
    // all in place.
    Err(SdnParseError::Xml(SerdeXmlRsMinFailure(
        "TODO-014-OFAC: XML deserialisation deferred to follow-up; \
         the surrounding ingest flow (digest, sanity check, log, \
         upsert) is exercised by `parse_sdn_count_only` for now"
            .into(),
    )))
}

/// Lightweight pre-OFAC-XML-wiring helper. Counts `<sdnEntry>` open
/// tags as a stand-in for the row count. NOT a parser — this is the
/// minimum signal the sanity-check needs.
pub fn parse_sdn_count_only(bytes: &[u8]) -> Result<u64, SdnParseError> {
    if bytes.is_empty() {
        return Err(SdnParseError::Empty);
    }
    let s = std::str::from_utf8(bytes).unwrap_or("");
    let count = s.matches("<sdnEntry").count() as u64;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_only_counts_entries() {
        let xml = b"<sdnList>
            <sdnEntry uid=\"1\">...</sdnEntry>
            <sdnEntry uid=\"2\">...</sdnEntry>
            <sdnEntry uid=\"3\">...</sdnEntry>
        </sdnList>";
        assert_eq!(parse_sdn_count_only(xml).unwrap(), 3);
    }

    #[test]
    fn empty_input_errors() {
        assert!(parse_sdn_count_only(b"").is_err());
        assert!(parse_sdn(b"").is_err());
    }

    #[test]
    fn count_only_zero_for_no_entries() {
        let xml = b"<sdnList></sdnList>";
        assert_eq!(parse_sdn_count_only(xml).unwrap(), 0);
    }
}
