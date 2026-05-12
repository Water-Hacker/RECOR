//! Serde helpers for domain types. Mirrors
//! `services/declaration/src/domain/serde_helpers.rs` so the two services'
//! events round-trip through the same JSON shape on the wire.

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

pub mod iso_date_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::format_description::well_known::Iso8601;
    use time::Date;

    pub fn serialize<S>(opt: &Option<Date>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt {
            Some(d) => d
                .format(&Iso8601::DATE)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(s) => Date::parse(&s, &Iso8601::DATE)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

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
