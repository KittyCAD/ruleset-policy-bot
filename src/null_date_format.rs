use std::convert::TryFrom;

use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use serde::{self, Deserialize, Deserializer, de, de::Error as _};
use serde_json::Value;

// The signature of a deserialize_with function must follow the pattern:
//
//    fn deserialize<'de, D>(D) -> Result<T, D::Error>
//    where
//        D: Deserializer<'de>
//
// although it may also be generic over the output types T.
pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;

    match value {
        Value::Null => Ok(default_datetime()),
        Value::String(s) => parse_from_string::<D::Error>(&s),
        Value::Number(num) => parse_from_number::<D::Error>(num),
        Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
            Err(D::Error::custom("expected a string or integer timestamp"))
        }
    }
}

fn default_datetime() -> DateTime<Utc> {
    let naive = crate::default_date().and_time(NaiveTime::MIN);
    Utc.from_utc_datetime(&naive)
}

fn parse_from_string<E>(value: &str) -> Result<DateTime<Utc>, E>
where
    E: de::Error,
{
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(default_datetime());
    }

    if let Ok(timestamp) = trimmed.parse::<i64>() {
        return parse_from_timestamp::<E>(timestamp);
    }

    DateTime::parse_from_str(trimmed, "%+")
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| E::custom(err.to_string()))
}

fn parse_from_number<E>(num: serde_json::Number) -> Result<DateTime<Utc>, E>
where
    E: de::Error,
{
    if let Some(value) = num.as_i64() {
        return parse_from_timestamp::<E>(value);
    }

    if let Some(value) = num.as_u64() {
        let timestamp = i64::try_from(value)
            .map_err(|_| E::custom(format!("unix timestamp out of range: {value}")))?;
        return parse_from_timestamp::<E>(timestamp);
    }

    Err(E::custom("expected integer timestamp"))
}

fn parse_from_timestamp<E>(timestamp: i64) -> Result<DateTime<Utc>, E>
where
    E: de::Error,
{
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| E::custom(format!("unix timestamp out of range: {timestamp}")))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::Deserialize;
    use serde_json::json;

    use crate::null_date_format::deserialize;

    #[derive(Deserialize)]
    struct DeserializableDateTime {
        #[serde(deserialize_with = "deserialize")]
        timestamp: DateTime<Utc>,
    }

    #[test]
    fn deserialize_handles_rfc3339_strings() {
        let value = json!({"timestamp": "2024-05-01T12:34:56Z"});
        let parsed: DeserializableDateTime = serde_json::from_value(value).unwrap();

        let expected = Utc.with_ymd_and_hms(2024, 5, 1, 12, 34, 56).unwrap();
        assert_eq!(parsed.timestamp, expected);
    }

    #[test]
    fn deserialize_accepts_unix_timestamp_numbers() {
        let value = json!({"timestamp": 1_700_000_000});
        let parsed: DeserializableDateTime = serde_json::from_value(value).unwrap();

        let expected = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap();
        assert_eq!(parsed.timestamp, expected);
    }

    #[test]
    fn deserialize_accepts_unix_timestamp_strings() {
        let value = json!({"timestamp": "1700000000"});
        let parsed: DeserializableDateTime = serde_json::from_value(value).unwrap();

        let expected = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap();
        assert_eq!(parsed.timestamp, expected);
    }

    #[test]
    fn deserialize_defaults_on_null_or_empty_values() {
        let null_value = json!({"timestamp": null});
        let empty_string = json!({"timestamp": "   "});

        let parsed_null: DeserializableDateTime = serde_json::from_value(null_value).unwrap();
        let parsed_empty: DeserializableDateTime = serde_json::from_value(empty_string).unwrap();

        let expected =
            Utc.from_utc_datetime(&crate::default_date().and_time(chrono::NaiveTime::MIN));
        assert_eq!(parsed_null.timestamp, expected);
        assert_eq!(parsed_empty.timestamp, expected);
    }

    #[test]
    fn deserialize_rejects_invalid_strings() {
        let value = json!({"timestamp": "not a date"});
        let result: Result<DeserializableDateTime, _> = serde_json::from_value(value);

        assert!(result.is_err());
    }
}
