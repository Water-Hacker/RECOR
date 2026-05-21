//! TODO-014-UN — UN Consolidated Security Council sanctions feed.
//!
//! Source: <https://scsanctions.un.org/resources/xml/en/consolidated.xml>
//!
//! Shape (UN v1):
//!
//! ```xml
//! <CONSOLIDATED_LIST dateGenerated="2026-04-12T01:00:00Z">
//!   <INDIVIDUALS>
//!     <INDIVIDUAL>
//!       <DATAID>6908123</DATAID>
//!       <REFERENCE_NUMBER>QDi.123</REFERENCE_NUMBER>
//!       <FIRST_NAME>JOHN</FIRST_NAME>
//!       <SECOND_NAME>HENRY</SECOND_NAME>
//!       <THIRD_NAME>DOE</THIRD_NAME>
//!       <UN_LIST_TYPE>Al-Qaida</UN_LIST_TYPE>
//!       <LISTED_ON>2010-06-12</LISTED_ON>
//!       <NATIONALITY>
//!         <VALUE>Cameroon</VALUE>
//!       </NATIONALITY>
//!       <INDIVIDUAL_ALIAS>
//!         <QUALITY>Good</QUALITY>
//!         <ALIAS_NAME>JONATHAN DOE</ALIAS_NAME>
//!       </INDIVIDUAL_ALIAS>
//!       <INDIVIDUAL_DATE_OF_BIRTH>
//!         <TYPE_OF_DATE>EXACT</TYPE_OF_DATE>
//!         <YEAR>1980</YEAR>
//!         <MONTH>7</MONTH>
//!         <DAY>15</DAY>
//!       </INDIVIDUAL_DATE_OF_BIRTH>
//!     </INDIVIDUAL>
//!   </INDIVIDUALS>
//!   <ENTITIES>...</ENTITIES>  <!-- skipped -->
//! </CONSOLIDATED_LIST>
//! ```

use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;

use crate::canonical::canonicalise_name;

#[derive(Debug, Error)]
pub enum UnParseError {
    #[error("XML parse failure: {0}")]
    Xml(String),
    #[error("empty UN feed (no bytes)")]
    Empty,
    #[error("malformed UN feed: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone)]
pub struct UnEntry {
    pub dataid: String,
    pub primary_name: String,
    pub aliases: Vec<String>,
    pub nationality: Option<String>,
    pub date_of_birth: Option<String>,
    pub sanction_program: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct ConsolidatedList {
    #[serde(default)]
    individuals: Option<Individuals>,
}

#[derive(Debug, Deserialize)]
struct Individuals {
    #[serde(rename = "INDIVIDUAL", default)]
    rows: Vec<RawIndividual>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct RawIndividual {
    dataid: String,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    second_name: Option<String>,
    #[serde(default)]
    third_name: Option<String>,
    #[serde(default)]
    fourth_name: Option<String>,
    #[serde(default, rename = "UN_LIST_TYPE")]
    un_list_type: Option<String>,
    #[serde(default, rename = "NATIONALITY")]
    nationality: Option<NationalityWrap>,
    #[serde(default, rename = "INDIVIDUAL_ALIAS")]
    aliases: Vec<IndividualAlias>,
    #[serde(default, rename = "INDIVIDUAL_DATE_OF_BIRTH")]
    dob: Option<IndividualDob>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct NationalityWrap {
    #[serde(default, rename = "VALUE")]
    value: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct IndividualAlias {
    #[serde(default)]
    alias_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct IndividualDob {
    #[serde(default)]
    year: Option<u16>,
    #[serde(default)]
    month: Option<u8>,
    #[serde(default)]
    day: Option<u8>,
}

pub fn parse_un(bytes: &[u8]) -> Result<Vec<UnEntry>, UnParseError> {
    if bytes.is_empty() {
        return Err(UnParseError::Empty);
    }
    let list: ConsolidatedList = quick_xml::de::from_reader(bytes)
        .map_err(|e| UnParseError::Xml(e.to_string()))?;
    let rows = match list.individuals {
        Some(i) => i.rows,
        None => return Ok(Vec::new()),
    };

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let combined = [
            r.first_name.as_deref().unwrap_or(""),
            r.second_name.as_deref().unwrap_or(""),
            r.third_name.as_deref().unwrap_or(""),
            r.fourth_name.as_deref().unwrap_or(""),
        ]
        .join(" ");
        let primary_name = canonicalise_name(&combined);
        if primary_name.is_empty() {
            return Err(UnParseError::Malformed(format!(
                "INDIVIDUAL dataid={} has no name parts",
                r.dataid
            )));
        }
        let aliases: Vec<String> = r
            .aliases
            .iter()
            .filter_map(|a| a.alias_name.as_deref())
            .map(canonicalise_name)
            .filter(|s| !s.is_empty())
            .collect();
        let nationality = r
            .nationality
            .as_ref()
            .and_then(|n| n.value.first())
            .and_then(|raw| country_to_iso2(raw));
        let date_of_birth = r.dob.and_then(|d| match (d.year, d.month, d.day) {
            (Some(y), Some(m), Some(day))
                if (1900..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&day) =>
            {
                Some(format!("{y:04}-{m:02}-{day:02}"))
            }
            _ => None,
        });
        let sanction_program = r
            .un_list_type
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "UNSPECIFIED".to_string());
        out.push(UnEntry {
            dataid: r.dataid,
            primary_name,
            aliases,
            nationality,
            date_of_birth,
            sanction_program,
        });
    }
    Ok(out)
}

fn country_to_iso2(name: &str) -> Option<String> {
    let lower = name.trim().to_ascii_lowercase();
    let code = match lower.as_str() {
        "cameroon" => "CM",
        "afghanistan" => "AF",
        "russian federation" | "russia" => "RU",
        "iran (islamic republic of)" | "iran" => "IR",
        "democratic people's republic of korea" | "north korea" => "KP",
        "syrian arab republic" | "syria" => "SY",
        "libya" => "LY",
        "yemen" => "YE",
        "somalia" => "SO",
        "sudan" => "SD",
        "south sudan" => "SS",
        "iraq" => "IQ",
        "central african republic" => "CF",
        "democratic republic of the congo" => "CD",
        "mali" => "ML",
        "lebanon" => "LB",
        _ => return None,
    };
    Some(code.to_string())
}

pub async fn upsert_un_entries(
    pool: &PgPool,
    entries: &[UnEntry],
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
            VALUES ('un_consolidated', $1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (source, source_id) DO UPDATE SET
                full_name_canonical = EXCLUDED.full_name_canonical,
                full_name_aliases   = EXCLUDED.full_name_aliases,
                nationality         = EXCLUDED.nationality,
                date_of_birth       = EXCLUDED.date_of_birth,
                sanction_program    = EXCLUDED.sanction_program,
                updated_at          = NOW()
            "#,
        )
        .bind(&e.dataid)
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

    const HAPPY: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<CONSOLIDATED_LIST dateGenerated="2026-04-12T01:00:00Z">
  <INDIVIDUALS>
    <INDIVIDUAL>
      <DATAID>6908123</DATAID>
      <FIRST_NAME>JOHN</FIRST_NAME>
      <SECOND_NAME>HENRY</SECOND_NAME>
      <THIRD_NAME>DOE</THIRD_NAME>
      <UN_LIST_TYPE>Al-Qaida</UN_LIST_TYPE>
      <NATIONALITY>
        <VALUE>Cameroon</VALUE>
      </NATIONALITY>
      <INDIVIDUAL_ALIAS>
        <QUALITY>Good</QUALITY>
        <ALIAS_NAME>JONATHAN DOE</ALIAS_NAME>
      </INDIVIDUAL_ALIAS>
      <INDIVIDUAL_DATE_OF_BIRTH>
        <TYPE_OF_DATE>EXACT</TYPE_OF_DATE>
        <YEAR>1980</YEAR>
        <MONTH>7</MONTH>
        <DAY>15</DAY>
      </INDIVIDUAL_DATE_OF_BIRTH>
    </INDIVIDUAL>
    <INDIVIDUAL>
      <DATAID>6908124</DATAID>
      <FIRST_NAME>JANE</FIRST_NAME>
      <SECOND_NAME>SMITH</SECOND_NAME>
      <UN_LIST_TYPE>Libya</UN_LIST_TYPE>
    </INDIVIDUAL>
  </INDIVIDUALS>
</CONSOLIDATED_LIST>"#;

    #[test]
    fn happy_path_parses_individuals() {
        let out = parse_un(HAPPY).expect("parse");
        assert_eq!(out.len(), 2);
        let john = &out[0];
        assert_eq!(john.dataid, "6908123");
        assert_eq!(john.primary_name, "john henry doe");
        assert_eq!(john.aliases, vec!["jonathan doe"]);
        assert_eq!(john.nationality.as_deref(), Some("CM"));
        assert_eq!(john.date_of_birth.as_deref(), Some("1980-07-15"));
        assert_eq!(john.sanction_program, "Al-Qaida");
        assert_eq!(out[1].dataid, "6908124");
        assert_eq!(out[1].primary_name, "jane smith");
    }

    #[test]
    fn empty_bytes_errors() {
        assert!(matches!(parse_un(b""), Err(UnParseError::Empty)));
    }

    #[test]
    fn empty_list_is_ok() {
        let xml = b"<CONSOLIDATED_LIST></CONSOLIDATED_LIST>";
        let out = parse_un(xml).expect("parse");
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_xml_errors() {
        let xml = b"<CONSOLIDATED_LIST><INDIVIDUALS>";
        assert!(matches!(parse_un(xml), Err(UnParseError::Xml(_))));
    }

    #[test]
    fn individual_without_names_errors() {
        let xml = br#"<CONSOLIDATED_LIST>
          <INDIVIDUALS>
            <INDIVIDUAL>
              <DATAID>1</DATAID>
              <UN_LIST_TYPE>Al-Qaida</UN_LIST_TYPE>
            </INDIVIDUAL>
          </INDIVIDUALS>
        </CONSOLIDATED_LIST>"#;
        assert!(matches!(parse_un(xml), Err(UnParseError::Malformed(_))));
    }
}
