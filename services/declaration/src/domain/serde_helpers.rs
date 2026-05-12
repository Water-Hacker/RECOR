//! Serde helpers for domain types. `time::Date` does not have a
//! default serde impl that round-trips through ISO-8601 `YYYY-MM-DD`
//! strings; this module provides one.

pub mod iso_date {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::format_description::well_known::Iso8601;
    use time::Date;

    pub fn serialize<S>(date: &Date, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        date.format(&Iso8601::DATE)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Date::parse(&s, &Iso8601::DATE).map_err(serde::de::Error::custom)
    }
}

/// ISO-8601 `OffsetDateTime` serde, e.g. `"2026-05-11T22:39:52.447Z"`.
/// Without this, the default `time::OffsetDateTime` serde emits a 9-
/// element array of date/time components, which is not a useful wire
/// format.
pub mod iso_datetime {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::format_description::well_known::Iso8601;
    use time::OffsetDateTime;

    pub fn serialize<S>(odt: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        odt.format(&Iso8601::DEFAULT)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        OffsetDateTime::parse(&s, &Iso8601::DEFAULT).map_err(serde::de::Error::custom)
    }
}

/// ISO-8601 `Option<OffsetDateTime>` serde — emits the string form on
/// `Some(_)`, `null` on `None`. Used for projection fields that are
/// populated late (e.g. `verified_at` only after the verification engine
/// has written back).
pub mod iso_datetime_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::format_description::well_known::Iso8601;
    use time::OffsetDateTime;

    pub fn serialize<S>(opt: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt {
            Some(odt) => odt
                .format(&Iso8601::DEFAULT)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(s) => OffsetDateTime::parse(&s, &Iso8601::DEFAULT)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use time::macros::date;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Wrapper {
        #[serde(with = "super::iso_date")]
        d: time::Date,
    }

    #[test]
    fn round_trip() {
        let w = Wrapper { d: date!(2026 - 05 - 11) };
        let s = serde_json::to_string(&w).unwrap();
        assert_eq!(s, r#"{"d":"2026-05-11"}"#);
        let parsed: Wrapper = serde_json::from_str(&s).unwrap();
        assert_eq!(w, parsed);
    }

    #[test]
    fn parses_iso_date_string() {
        let parsed: Wrapper = serde_json::from_str(r#"{"d":"2026-01-01"}"#).unwrap();
        assert_eq!(parsed.d, date!(2026 - 01 - 01));
    }
}
