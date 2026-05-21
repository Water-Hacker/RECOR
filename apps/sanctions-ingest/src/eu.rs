//! TODO-014-EU — EU CFSP consolidated financial sanctions feed.
//!
//! Source: <https://webgate.ec.europa.eu/fsd/fsf/public/files/xmlFullSanctionsList_1_1/content>
//!
//! Shape (CFSP v1.1):
//!
//! ```xml
//! <export>
//!   <sanctionEntity logicalId="123" euReferenceNumber="EU.123.45">
//!     <subjectType code="P" classificationCode="person"/>
//!     <nameAlias firstName="JOHN" lastName="DOE" function="primary"/>
//!     <nameAlias firstName="JONATHAN" lastName="DOE" function="alias"/>
//!     <citizenship countryDescription="Cameroon" countryIso2Code="CM"/>
//!     <birthdate birthdate="1980-07-15"/>
//!     <regulation programme="CFSP_REGIME_1"/>
//!   </sanctionEntity>
//! </export>
//! ```
//!
//! Only `subjectType.code == "P"` rows are individuals; group / entity
//! / ship rows are skipped.

use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;

use crate::canonical::canonicalise_name;

#[derive(Debug, Error)]
pub enum EuParseError {
    #[error("XML parse failure: {0}")]
    Xml(String),
    #[error("empty EU feed (no bytes)")]
    Empty,
    #[error("malformed EU feed: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone)]
pub struct EuEntry {
    pub logical_id: String,
    pub primary_name: String,
    pub aliases: Vec<String>,
    pub nationality: Option<String>,
    pub date_of_birth: Option<String>,
    pub sanction_program: String,
}

// ─── XML model ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Export {
    #[serde(rename = "sanctionEntity", default)]
    entities: Vec<RawEntity>,
}

#[derive(Debug, Deserialize)]
struct RawEntity {
    #[serde(rename = "@logicalId", default)]
    logical_id: Option<String>,
    #[serde(rename = "@euReferenceNumber", default)]
    eu_ref: Option<String>,
    #[serde(default, rename = "subjectType")]
    subject_type: Option<SubjectType>,
    #[serde(default, rename = "nameAlias")]
    name_aliases: Vec<NameAlias>,
    #[serde(default)]
    citizenship: Vec<Citizenship>,
    #[serde(default)]
    birthdate: Vec<Birthdate>,
    #[serde(default)]
    regulation: Vec<Regulation>,
}

#[derive(Debug, Deserialize)]
struct SubjectType {
    #[serde(rename = "@code", default)]
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NameAlias {
    #[serde(rename = "@firstName", default)]
    first_name: Option<String>,
    #[serde(rename = "@lastName", default)]
    last_name: Option<String>,
    #[serde(rename = "@wholeName", default)]
    whole_name: Option<String>,
    #[serde(rename = "@function", default)]
    function: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Citizenship {
    #[serde(rename = "@countryIso2Code", default)]
    iso2: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Birthdate {
    #[serde(rename = "@birthdate", default)]
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Regulation {
    #[serde(rename = "@programme", default)]
    programme: Option<String>,
}

// ─── Parser ───────────────────────────────────────────────────────────

pub fn parse_eu(bytes: &[u8]) -> Result<Vec<EuEntry>, EuParseError> {
    if bytes.is_empty() {
        return Err(EuParseError::Empty);
    }
    let export: Export = quick_xml::de::from_reader(bytes)
        .map_err(|e| EuParseError::Xml(e.to_string()))?;

    let mut out = Vec::with_capacity(export.entities.len());
    for raw in export.entities {
        // Only persons; code "P" per CFSP convention.
        if let Some(st) = &raw.subject_type {
            if let Some(code) = st.code.as_deref() {
                if !code.eq_ignore_ascii_case("P") {
                    continue;
                }
            }
        }
        // Resolve a stable source id: prefer logicalId, fall back to euRef.
        let source_id = raw
            .logical_id
            .clone()
            .or(raw.eu_ref.clone())
            .ok_or_else(|| {
                EuParseError::Malformed(
                    "sanctionEntity has no logicalId or euReferenceNumber".to_string(),
                )
            })?;

        let primary_alias = raw
            .name_aliases
            .iter()
            .find(|n| {
                n.function
                    .as_deref()
                    .map(|s| s.eq_ignore_ascii_case("primary"))
                    .unwrap_or(false)
            })
            .or_else(|| raw.name_aliases.first());
        let primary_name = match primary_alias {
            Some(n) => alias_to_name(n),
            None => {
                return Err(EuParseError::Malformed(format!(
                    "sanctionEntity logicalId={source_id} has no nameAlias"
                )));
            }
        };
        if primary_name.is_empty() {
            return Err(EuParseError::Malformed(format!(
                "sanctionEntity logicalId={source_id} primary nameAlias is empty"
            )));
        }
        let aliases: Vec<String> = raw
            .name_aliases
            .iter()
            .filter(|n| {
                !n.function
                    .as_deref()
                    .map(|s| s.eq_ignore_ascii_case("primary"))
                    .unwrap_or(false)
            })
            .map(alias_to_name)
            .filter(|n| !n.is_empty())
            .collect();
        let nationality = raw
            .citizenship
            .iter()
            .find_map(|c| c.iso2.as_deref())
            .map(|s| s.to_ascii_uppercase());
        let date_of_birth = raw
            .birthdate
            .iter()
            .find_map(|b| b.date.as_deref())
            .map(|s| s.to_string());
        let sanction_program = {
            let progs: Vec<String> = raw
                .regulation
                .iter()
                .filter_map(|r| r.programme.clone())
                .collect();
            if progs.is_empty() {
                "UNSPECIFIED".to_string()
            } else {
                progs.join(",")
            }
        };
        out.push(EuEntry {
            logical_id: source_id,
            primary_name,
            aliases,
            nationality,
            date_of_birth,
            sanction_program,
        });
    }
    Ok(out)
}

fn alias_to_name(n: &NameAlias) -> String {
    let raw = if let Some(w) = n.whole_name.as_deref() {
        if !w.trim().is_empty() {
            w.to_string()
        } else {
            join(n.first_name.as_deref(), n.last_name.as_deref())
        }
    } else {
        join(n.first_name.as_deref(), n.last_name.as_deref())
    };
    canonicalise_name(&raw)
}

fn join(a: Option<&str>, b: Option<&str>) -> String {
    match (a, b) {
        (Some(x), Some(y)) => format!("{x} {y}"),
        (Some(x), None) => x.to_string(),
        (None, Some(y)) => y.to_string(),
        (None, None) => String::new(),
    }
}

// ─── Upsert ───────────────────────────────────────────────────────────

pub async fn upsert_eu_entries(
    pool: &PgPool,
    entries: &[EuEntry],
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut applied: u64 = 0;
    for e in entries {
        let aliases_json = serde_json::Value::Array(
            e.aliases
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
        );
        let dob_value: Option<sqlx::types::time::Date> = e
            .date_of_birth
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
            INSERT INTO sanctions_persons (
                source, source_id, full_name_canonical, full_name_aliases,
                nationality, date_of_birth, sanction_program,
                created_at, updated_at
            )
            VALUES ('eu_cfsp', $1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (source, source_id) DO UPDATE SET
                full_name_canonical = EXCLUDED.full_name_canonical,
                full_name_aliases   = EXCLUDED.full_name_aliases,
                nationality         = EXCLUDED.nationality,
                date_of_birth       = EXCLUDED.date_of_birth,
                sanction_program    = EXCLUDED.sanction_program,
                updated_at          = NOW()
            "#,
        )
        .bind(&e.logical_id)
        .bind(&e.primary_name)
        .bind(&aliases_json)
        .bind(e.nationality.as_deref())
        .bind(dob_value)
        .bind(&e.sanction_program)
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

    // NOTE: not a raw-byte literal; raw-byte literals are ASCII-only,
    // and we deliberately include diacritics here to exercise the
    // canonicaliser. `.as_bytes()` lifts the UTF-8 string-literal into
    // the byte slice the parser consumes.
    const HAPPY: &str = "\
<?xml version=\"1.0\" encoding=\"utf-8\"?>
<export>
  <sanctionEntity logicalId=\"500\" euReferenceNumber=\"EU.500.1\">
    <subjectType code=\"P\"/>
    <nameAlias firstName=\"JOHN\" lastName=\"DOE\" function=\"primary\"/>
    <nameAlias firstName=\"JONATHAN\" lastName=\"DOE\" function=\"alias\"/>
    <citizenship countryIso2Code=\"CM\"/>
    <birthdate birthdate=\"1980-07-15\"/>
    <regulation programme=\"CFSP_LIBYA\"/>
  </sanctionEntity>
  <sanctionEntity logicalId=\"501\">
    <subjectType code=\"E\"/>
    <nameAlias wholeName=\"ACME ENT LTD\"/>
  </sanctionEntity>
  <sanctionEntity logicalId=\"502\">
    <subjectType code=\"P\"/>
    <nameAlias wholeName=\"Maria José Müller\" function=\"primary\"/>
  </sanctionEntity>
</export>";

    #[test]
    fn happy_parses_persons_skips_entities() {
        let out = parse_eu(HAPPY.as_bytes()).expect("parse");
        assert_eq!(out.len(), 2);
        let john = &out[0];
        assert_eq!(john.logical_id, "500");
        assert_eq!(john.primary_name, "john doe");
        assert_eq!(john.aliases, vec!["jonathan doe"]);
        assert_eq!(john.nationality.as_deref(), Some("CM"));
        assert_eq!(john.date_of_birth.as_deref(), Some("1980-07-15"));
        assert_eq!(john.sanction_program, "CFSP_LIBYA");
        let maria = &out[1];
        assert_eq!(maria.primary_name, "maria jose muller");
    }

    #[test]
    fn empty_bytes_errors() {
        assert!(matches!(parse_eu(b""), Err(EuParseError::Empty)));
    }

    #[test]
    fn empty_export_is_ok() {
        let xml = b"<export></export>";
        let out = parse_eu(xml).expect("parse");
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_xml_errors() {
        let xml = b"<export><sanctionEntity";
        assert!(matches!(parse_eu(xml), Err(EuParseError::Xml(_))));
    }

    #[test]
    fn entity_missing_alias_errors() {
        let xml = br#"<export>
          <sanctionEntity logicalId="600">
            <subjectType code="P"/>
          </sanctionEntity>
        </export>"#;
        let out = parse_eu(xml);
        assert!(matches!(out, Err(EuParseError::Malformed(_))));
    }
}
