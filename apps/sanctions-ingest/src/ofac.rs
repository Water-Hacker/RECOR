//! TODO-014-OFAC — OFAC SDN feed parser + upsert.
//!
//! Source: <https://www.treasury.gov/ofac/downloads/sdn.xml>
//!
//! Shape (OFAC SDN v1):
//!
//! ```xml
//! <sdnList xmlns="http://tempuri.org/sdnList.xsd">
//!   <publshInformation>
//!     <Publish_Date>04/12/2026</Publish_Date>
//!     <Record_Count>15234</Record_Count>
//!   </publshInformation>
//!   <sdnEntry>
//!     <uid>12345</uid>
//!     <firstName>JOHN</firstName>
//!     <lastName>DOE</lastName>
//!     <sdnType>Individual</sdnType>
//!     <programList><program>SDGT</program></programList>
//!     <akaList><aka><firstName>JON</firstName><lastName>DOE</lastName></aka></akaList>
//!     <nationalityList><nationality><country>Cameroon</country><mainEntry>true</mainEntry></nationality></nationalityList>
//!     <dateOfBirthList><dateOfBirthItem><dateOfBirth>15 Jul 1980</dateOfBirth><mainEntry>true</mainEntry></dateOfBirthItem></dateOfBirthList>
//!   </sdnEntry>
//! </sdnList>
//! ```
//!
//! Only `sdnType=Individual` rows are persisted to `sanctions_persons`;
//! `Entity` / `Vessel` / `Aircraft` rows are skipped — entity-side
//! screening lives outside TODO-014's scope.

use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;

use crate::canonical::canonicalise_name;

#[derive(Debug, Error)]
pub enum SdnParseError {
    #[error("XML parse failure: {0}")]
    Xml(String),
    #[error("empty SDN feed (no bytes)")]
    Empty,
    #[error("malformed SDN feed: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone)]
pub struct SdnEntry {
    pub uid: String,
    pub primary_name: String,
    pub aliases: Vec<String>,
    pub nationality: Option<String>,
    pub date_of_birth: Option<String>,
    pub sanction_program: String,
}

// ─── XML model ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SdnList {
    #[serde(rename = "sdnEntry", default)]
    entries: Vec<RawSdnEntry>,
}

#[derive(Debug, Deserialize)]
struct RawSdnEntry {
    uid: String,
    #[serde(default, rename = "firstName")]
    first_name: Option<String>,
    #[serde(default, rename = "lastName")]
    last_name: Option<String>,
    #[serde(default, rename = "sdnType")]
    sdn_type: Option<String>,
    #[serde(default, rename = "programList")]
    program_list: Option<ProgramList>,
    #[serde(default, rename = "akaList")]
    aka_list: Option<AkaList>,
    #[serde(default, rename = "nationalityList")]
    nationality_list: Option<NationalityList>,
    #[serde(default, rename = "dateOfBirthList")]
    dob_list: Option<DateOfBirthList>,
}

#[derive(Debug, Deserialize)]
struct ProgramList {
    #[serde(rename = "program", default)]
    programs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AkaList {
    #[serde(rename = "aka", default)]
    akas: Vec<Aka>,
}

#[derive(Debug, Deserialize)]
struct Aka {
    #[serde(default, rename = "firstName")]
    first_name: Option<String>,
    #[serde(default, rename = "lastName")]
    last_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NationalityList {
    #[serde(rename = "nationality", default)]
    items: Vec<Nationality>,
}

#[derive(Debug, Deserialize)]
struct Nationality {
    #[serde(default)]
    country: Option<String>,
    #[serde(default, rename = "mainEntry")]
    main_entry: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DateOfBirthList {
    #[serde(rename = "dateOfBirthItem", default)]
    items: Vec<DateOfBirthItem>,
}

#[derive(Debug, Deserialize)]
struct DateOfBirthItem {
    #[serde(default, rename = "dateOfBirth")]
    date_of_birth: Option<String>,
    #[serde(default, rename = "mainEntry")]
    main_entry: Option<String>,
}

// ─── Parser ───────────────────────────────────────────────────────────

pub fn parse_sdn(bytes: &[u8]) -> Result<Vec<SdnEntry>, SdnParseError> {
    if bytes.is_empty() {
        return Err(SdnParseError::Empty);
    }
    let raw: SdnList = quick_xml::de::from_reader(bytes)
        .map_err(|e| SdnParseError::Xml(e.to_string()))?;

    let mut out = Vec::with_capacity(raw.entries.len());
    for r in raw.entries {
        if let Some(t) = r.sdn_type.as_deref() {
            if !t.eq_ignore_ascii_case("Individual") {
                continue;
            }
        }
        let primary = join_name(r.first_name.as_deref(), r.last_name.as_deref());
        if primary.is_empty() {
            return Err(SdnParseError::Malformed(format!(
                "sdnEntry uid={} has no first/last name",
                r.uid
            )));
        }
        let aliases: Vec<String> = r
            .aka_list
            .map(|al| {
                al.akas
                    .into_iter()
                    .map(|a| join_name(a.first_name.as_deref(), a.last_name.as_deref()))
                    .filter(|n| !n.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let nationality = r.nationality_list.and_then(|nl| pick_main_nationality(&nl));
        let date_of_birth = r.dob_list.and_then(|dl| pick_main_dob(&dl));
        let sanction_program = r
            .program_list
            .map(|pl| pl.programs.join(","))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "UNSPECIFIED".to_string());
        out.push(SdnEntry {
            uid: r.uid,
            primary_name: primary,
            aliases,
            nationality,
            date_of_birth,
            sanction_program,
        });
    }
    Ok(out)
}

fn join_name(first: Option<&str>, last: Option<&str>) -> String {
    let raw = match (first, last) {
        (Some(f), Some(l)) => format!("{f} {l}"),
        (Some(f), None) => f.to_string(),
        (None, Some(l)) => l.to_string(),
        (None, None) => String::new(),
    };
    canonicalise_name(&raw)
}

fn pick_main_nationality(nl: &NationalityList) -> Option<String> {
    let item = nl
        .items
        .iter()
        .find(|n| {
            n.main_entry
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        })
        .or_else(|| nl.items.first())?;
    let country = item.country.as_deref()?;
    country_to_iso2(country).map(|s| s.to_string())
}

fn pick_main_dob(dl: &DateOfBirthList) -> Option<String> {
    let item = dl
        .items
        .iter()
        .find(|d| {
            d.main_entry
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        })
        .or_else(|| dl.items.first())?;
    parse_ofac_dob(item.date_of_birth.as_deref()?)
}

fn parse_ofac_dob(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.trim().split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let day: u8 = parts[0].parse().ok()?;
    let month = match parts[1].to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let year: u16 = parts[2].parse().ok()?;
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn country_to_iso2(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    Some(match n.as_str() {
        "cameroon" => "CM",
        "united states" | "usa" | "united states of america" => "US",
        "united kingdom" | "uk" | "great britain" => "GB",
        "france" => "FR",
        "germany" => "DE",
        "nigeria" => "NG",
        "russia" | "russian federation" => "RU",
        "china" => "CN",
        "iran" => "IR",
        "north korea" | "korea, north" | "dprk" => "KP",
        "syria" => "SY",
        "venezuela" => "VE",
        "cuba" => "CU",
        _ => return None,
    })
}

// ─── Upsert ───────────────────────────────────────────────────────────

pub async fn upsert_sdn_entries(
    pool: &PgPool,
    entries: &[SdnEntry],
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
            VALUES ('ofac_sdn', $1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (source, source_id) DO UPDATE SET
                full_name_canonical = EXCLUDED.full_name_canonical,
                full_name_aliases   = EXCLUDED.full_name_aliases,
                nationality         = EXCLUDED.nationality,
                date_of_birth       = EXCLUDED.date_of_birth,
                sanction_program    = EXCLUDED.sanction_program,
                updated_at          = NOW()
            "#,
        )
        .bind(&e.uid)
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

    const HAPPY: &str = "\
<?xml version=\"1.0\" encoding=\"utf-8\"?>
<sdnList>
  <sdnEntry>
    <uid>1001</uid>
    <firstName>JOHN</firstName>
    <lastName>DOE</lastName>
    <sdnType>Individual</sdnType>
    <programList><program>SDGT</program><program>SDNT</program></programList>
    <akaList>
      <aka><firstName>JONATHAN</firstName><lastName>DOE</lastName></aka>
    </akaList>
    <nationalityList>
      <nationality><country>Cameroon</country><mainEntry>true</mainEntry></nationality>
    </nationalityList>
    <dateOfBirthList>
      <dateOfBirthItem><dateOfBirth>15 Jul 1980</dateOfBirth><mainEntry>true</mainEntry></dateOfBirthItem>
    </dateOfBirthList>
  </sdnEntry>
  <sdnEntry>
    <uid>2002</uid>
    <firstName>ACME</firstName>
    <lastName>HOLDINGS</lastName>
    <sdnType>Entity</sdnType>
  </sdnEntry>
</sdnList>";

    #[test]
    fn happy_path_parses_individual_only() {
        let entries = parse_sdn(HAPPY.as_bytes()).expect("parse ok");
        assert_eq!(entries.len(), 1, "Entity row must be skipped");
        let e = &entries[0];
        assert_eq!(e.uid, "1001");
        assert_eq!(e.primary_name, "john doe");
        assert_eq!(e.aliases, vec!["jonathan doe".to_string()]);
        assert_eq!(e.nationality.as_deref(), Some("CM"));
        assert_eq!(e.date_of_birth.as_deref(), Some("1980-07-15"));
        assert_eq!(e.sanction_program, "SDGT,SDNT");
    }

    #[test]
    fn empty_bytes_errors() {
        assert!(matches!(parse_sdn(b""), Err(SdnParseError::Empty)));
    }

    #[test]
    fn empty_feed_returns_empty_vec() {
        let entries = parse_sdn(b"<sdnList></sdnList>").expect("parse ok");
        assert!(entries.is_empty());
    }

    #[test]
    fn malformed_xml_errors() {
        assert!(matches!(
            parse_sdn(b"<sdnList><sdnEntry><uid>"),
            Err(SdnParseError::Xml(_))
        ));
    }

    #[test]
    fn missing_names_errors() {
        let xml = b"<sdnList><sdnEntry><uid>9</uid><sdnType>Individual</sdnType></sdnEntry></sdnList>";
        assert!(matches!(parse_sdn(xml), Err(SdnParseError::Malformed(_))));
    }
}
