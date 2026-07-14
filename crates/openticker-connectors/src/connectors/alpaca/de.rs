use serde::Deserialize;
use serde::de::{self, Deserializer};

pub(super) fn deserialize_f64_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(raw) => raw
            .parse::<f64>()
            .map_err(|_| de::Error::custom(format!("expected numeric string, received `{raw}`"))),
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| de::Error::custom("expected f64-compatible number")),
        _ => Err(de::Error::custom("expected string or number")),
    }
}

pub(super) fn deserialize_option_f64_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    let parsed = match value {
        serde_json::Value::String(raw) => raw
            .parse::<f64>()
            .map_err(|_| de::Error::custom(format!("expected numeric string, received `{raw}`")))?,
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| de::Error::custom("expected f64-compatible number"))?,
        serde_json::Value::Null => return Ok(None),
        _ => return Err(de::Error::custom("expected string, number, or null")),
    };

    if !parsed.is_finite() || parsed <= f64::EPSILON {
        return Ok(None);
    }
    Ok(Some(parsed))
}
