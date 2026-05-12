//! Wire-format helpers — ISO-8601 date + RFC-3339 datetime serialisation
//! shared by the events and DTOs.

pub mod iso_date {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::macros::format_description;
    use time::Date;

    pub fn serialize<S>(d: &Date, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let fmt = format_description!("[year]-[month]-[day]");
        let out = d
            .format(&fmt)
            .map_err(|e| serde::ser::Error::custom(e.to_string()))?;
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        let fmt = format_description!("[year]-[month]-[day]");
        Date::parse(&s, &fmt).map_err(serde::de::Error::custom)
    }
}

pub mod iso_datetime {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    pub fn serialize<S>(t: &OffsetDateTime, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let out = t
            .format(&Rfc3339)
            .map_err(|e| serde::ser::Error::custom(e.to_string()))?;
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        OffsetDateTime::parse(&s, &Rfc3339).map_err(serde::de::Error::custom)
    }
}

pub mod iso_date_opt {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::macros::format_description;
    use time::Date;

    pub fn serialize<S>(d: &Option<Date>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match d {
            None => s.serialize_none(),
            Some(date) => {
                let fmt = format_description!("[year]-[month]-[day]");
                let out = date
                    .format(&fmt)
                    .map_err(|e| serde::ser::Error::custom(e.to_string()))?;
                s.serialize_str(&out)
            }
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<String> = Option::deserialize(d)?;
        match raw {
            None => Ok(None),
            Some(s) => {
                let fmt = format_description!("[year]-[month]-[day]");
                Date::parse(&s, &fmt)
                    .map(Some)
                    .map_err(serde::de::Error::custom)
            }
        }
    }
}
