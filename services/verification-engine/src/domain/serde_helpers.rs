//! ISO-8601 serde helpers for `time::Date` and `time::OffsetDateTime`.
//! Match the helpers in services/declaration so the on-wire payloads
//! round-trip cleanly.

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
