//! TODO-014-ICIJ — ICIJ leak-set ingestion.
//!
//! ICIJ publishes its leak datasets (Offshore Leaks, Panama Papers,
//! Paradise Papers, Pandora Papers) as CSV-per-node-kind under
//! <https://offshoreleaks.icij.org/pages/database>. The four datasets
//! share a common header skeleton; we treat the dataset selection as
//! a runtime flag.
//!
//! Sub-modes (selected by `--dataset`):
//!   - `offshore_leaks` → `source = 'icij_offshore_leaks'`
//!   - `panama`         → `source = 'icij_panama'`
//!   - `paradise`       → `source = 'icij_paradise'`
//!   - `pandora`        → `source = 'icij_pandora'`
//!
//! Within each dataset the CSV is one row per node. The `node_kind`
//! column maps `node_id_type` → `person | officer | intermediary | entity`.
//! v1 of the verification engine consults `person` and `officer` rows;
//! `intermediary` and `entity` rows are still ingested so the table is
//! ready when adverse-media stage extends.
//!
//! Headers (common shape, observed across all four datasets):
//!
//! ```csv
//! node_id,name,countries,country_codes,sourceID,note,valid_until
//! 12001,"DOE, JOHN","Cameroon","CMR","Panama Papers - Mossack Fonseca","intermediary noted in leaks","2017-09-15"
//! ```
//!
//! `node_id_type` is occasionally absent; the binary's `--node-kind`
//! flag (defaulting to `person`) determines the value to land.

use std::str::FromStr;

use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;

use crate::canonical::canonicalise_name;

#[derive(Debug, Error)]
pub enum IcijParseError {
    #[error("CSV parse failure: {0}")]
    Csv(String),
    #[error("empty ICIJ feed (no bytes)")]
    Empty,
    #[error("malformed ICIJ row at line {line}: {message}")]
    Malformed { line: u64, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcijDataset {
    OffshoreLeaks,
    Panama,
    Paradise,
    Pandora,
}

impl IcijDataset {
    pub fn as_source(self) -> &'static str {
        match self {
            IcijDataset::OffshoreLeaks => "icij_offshore_leaks",
            IcijDataset::Panama => "icij_panama",
            IcijDataset::Paradise => "icij_paradise",
            IcijDataset::Pandora => "icij_pandora",
        }
    }
}

impl FromStr for IcijDataset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "offshore_leaks" => Ok(Self::OffshoreLeaks),
            "panama" => Ok(Self::Panama),
            "paradise" => Ok(Self::Paradise),
            "pandora" => Ok(Self::Pandora),
            other => Err(format!(
                "unknown ICIJ dataset `{other}` (expected one of: offshore_leaks, panama, paradise, pandora)"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Person,
    Officer,
    Intermediary,
    Entity,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Person => "person",
            NodeKind::Officer => "officer",
            NodeKind::Intermediary => "intermediary",
            NodeKind::Entity => "entity",
        }
    }
}

impl FromStr for NodeKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "person" => Ok(Self::Person),
            "officer" => Ok(Self::Officer),
            "intermediary" => Ok(Self::Intermediary),
            "entity" => Ok(Self::Entity),
            other => Err(format!(
                "unknown ICIJ node_kind `{other}` (expected one of: person, officer, intermediary, entity)"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IcijEntry {
    pub node_id: String,
    pub node_kind: NodeKind,
    pub primary_name: String,
    pub country_raw: Option<String>,
    pub snippet: Option<String>,
    pub leaked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRow {
    node_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    countries: Option<String>,
    #[serde(default, rename = "sourceID")]
    source_id_field: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    valid_until: Option<String>,
}

/// Parse an ICIJ leak CSV.
///
/// `default_node_kind` is the value to land on the `node_kind` column.
/// ICIJ's per-dataset CSVs are split by node kind already; the binary
/// passes the kind matching the file selected.
pub fn parse_icij(
    bytes: &[u8],
    default_node_kind: NodeKind,
) -> Result<Vec<IcijEntry>, IcijParseError> {
    if bytes.is_empty() {
        return Err(IcijParseError::Empty);
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(bytes);

    let mut out = Vec::new();
    for (idx, result) in rdr.deserialize::<RawRow>().enumerate() {
        let line = (idx as u64) + 2; // +1 for 1-based, +1 for header
        let raw: RawRow = result.map_err(|e| match e.kind() {
            csv::ErrorKind::Deserialize { .. } => IcijParseError::Malformed {
                line,
                message: e.to_string(),
            },
            _ => IcijParseError::Csv(e.to_string()),
        })?;
        let name = match raw.name.as_deref() {
            Some(n) if !n.trim().is_empty() => n,
            _ => {
                return Err(IcijParseError::Malformed {
                    line,
                    message: format!("row node_id={} has empty name", raw.node_id),
                });
            }
        };
        let primary_name = canonicalise_name(name);
        if primary_name.is_empty() {
            return Err(IcijParseError::Malformed {
                line,
                message: format!(
                    "row node_id={} canonical name is empty after normalisation",
                    raw.node_id
                ),
            });
        }
        let country_raw = raw
            .countries
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let snippet_parts: Vec<String> = [raw.source_id_field, raw.note]
            .into_iter()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let snippet = if snippet_parts.is_empty() {
            None
        } else {
            Some(snippet_parts.join(" | "))
        };
        let leaked_at = raw
            .valid_until
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .and_then(parse_iso_date);
        out.push(IcijEntry {
            node_id: raw.node_id,
            node_kind: default_node_kind,
            primary_name,
            country_raw,
            snippet,
            leaked_at,
        });
    }
    Ok(out)
}

/// Accept either `YYYY-MM-DD` or `YYYY/MM/DD`.
fn parse_iso_date(raw: &str) -> Option<String> {
    let normalised = raw.replace('/', "-");
    let parts: Vec<&str> = normalised.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: u16 = parts[0].parse().ok()?;
    let m: u8 = parts[1].parse().ok()?;
    let d: u8 = parts[2].parse().ok()?;
    if !(1900..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Upsert into `icij_persons`. The table's UNIQUE constraint is
/// `(source_dataset, source_id, node_kind)`.
pub async fn upsert_icij_entries(
    pool: &PgPool,
    dataset: IcijDataset,
    entries: &[IcijEntry],
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut applied: u64 = 0;
    for e in entries {
        let leaked_value: Option<sqlx::types::time::Date> = e
            .leaked_at
            .as_deref()
            .and_then(|d| {
                sqlx::types::time::Date::parse(
                    d,
                    &time::format_description::well_known::Iso8601::DATE,
                )
                .ok()
            });
        sqlx::query(
            r#"
            INSERT INTO icij_persons (
                node_kind, source_id, source_dataset,
                full_name_canonical, country_raw, snippet, leaked_at,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            ON CONFLICT (source_dataset, source_id, node_kind) DO UPDATE SET
                full_name_canonical = EXCLUDED.full_name_canonical,
                country_raw         = EXCLUDED.country_raw,
                snippet             = EXCLUDED.snippet,
                leaked_at           = EXCLUDED.leaked_at
            "#,
        )
        .bind(e.node_kind.as_str())
        .bind(&e.node_id)
        .bind(dataset.as_source())
        .bind(&e.primary_name)
        .bind(e.country_raw.as_deref())
        .bind(e.snippet.as_deref())
        .bind(leaked_value)
        .execute(&mut *tx)
        .await?;
        applied += 1;
    }
    tx.commit().await?;
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: not a raw-byte literal (raw-byte literals are ASCII-only);
    // diacritics here exercise the canonicaliser.
    const HAPPY: &str = "\
node_id,name,countries,sourceID,note,valid_until
12001,\"Doe, John\",\"Cameroon\",\"Panama Papers - Mossack Fonseca\",\"intermediary\",\"2017-09-15\"
12002,\"Müller, María\",\"Germany\",\"Panama Papers\",\"\",\"\"
12003,\"Smith, Jane\",\"\",,,
";

    #[test]
    fn happy_path_parses_three_rows() {
        let out = parse_icij(HAPPY.as_bytes(), NodeKind::Person).expect("parse");
        assert_eq!(out.len(), 3);
        let doe = &out[0];
        assert_eq!(doe.node_id, "12001");
        assert_eq!(doe.primary_name, "doe john");
        assert_eq!(doe.country_raw.as_deref(), Some("Cameroon"));
        assert_eq!(
            doe.snippet.as_deref(),
            Some("Panama Papers - Mossack Fonseca | intermediary")
        );
        assert_eq!(doe.leaked_at.as_deref(), Some("2017-09-15"));
        let muller = &out[1];
        assert_eq!(muller.primary_name, "muller maria");
        assert_eq!(muller.country_raw.as_deref(), Some("Germany"));
    }

    #[test]
    fn empty_bytes_errors() {
        assert!(matches!(
            parse_icij(b"", NodeKind::Person),
            Err(IcijParseError::Empty)
        ));
    }

    #[test]
    fn empty_csv_with_headers_only_is_ok() {
        let csv = b"node_id,name,countries,sourceID,note,valid_until\n";
        let out = parse_icij(csv, NodeKind::Person).expect("parse");
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_row_missing_name_errors() {
        let csv = b"node_id,name,countries,sourceID,note,valid_until\n99,,,,,\n";
        let out = parse_icij(csv, NodeKind::Person);
        assert!(matches!(out, Err(IcijParseError::Malformed { line: 2, .. })));
    }

    #[test]
    fn dataset_strings_round_trip() {
        for (s, expected_source) in [
            ("offshore_leaks", "icij_offshore_leaks"),
            ("panama", "icij_panama"),
            ("paradise", "icij_paradise"),
            ("pandora", "icij_pandora"),
        ] {
            let d: IcijDataset = s.parse().expect("known dataset");
            assert_eq!(d.as_source(), expected_source);
        }
        let bad: Result<IcijDataset, _> = "vatican".parse();
        assert!(bad.is_err());
    }

    #[test]
    fn large_csv_with_500_rows_parses() {
        let mut csv = String::from("node_id,name,countries,sourceID,note,valid_until\n");
        for i in 0..500 {
            csv.push_str(&format!(
                "{i},\"PERSON_{i}\",\"Cameroon\",\"Test\",\"\",\"\"\n"
            ));
        }
        let out = parse_icij(csv.as_bytes(), NodeKind::Officer).expect("parse");
        assert_eq!(out.len(), 500);
        assert_eq!(out[42].node_kind, NodeKind::Officer);
    }
}
