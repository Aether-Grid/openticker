use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

use crate::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    D1,
}

impl Timeframe {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1h",
            Self::H4 => "4h",
            Self::D1 => "1d",
        }
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        match self {
            Self::M1 => Duration::from_mins(1),
            Self::M5 => Duration::from_mins(5),
            Self::M15 => Duration::from_mins(15),
            Self::M30 => Duration::from_mins(30),
            Self::H1 => Duration::from_hours(1),
            Self::H4 => Duration::from_hours(4),
            Self::D1 => Duration::from_hours(24),
        }
    }
}

impl Display for Timeframe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Timeframe {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1m" => Ok(Self::M1),
            "5m" => Ok(Self::M5),
            "15m" => Ok(Self::M15),
            "30m" => Ok(Self::M30),
            "1h" => Ok(Self::H1),
            "4h" => Ok(Self::H4),
            "1d" => Ok(Self::D1),
            other => Err(CoreError::InvalidTimeframe(other.to_owned())),
        }
    }
}

impl Serialize for Timeframe {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Timeframe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Timeframe::from_str(raw.as_str()).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::Timeframe;
    use crate::CoreError;

    #[test]
    fn parses_timeframe() {
        assert_eq!(Timeframe::from_str("1m").unwrap(), Timeframe::M1);
        assert_eq!(Timeframe::from_str("4h").unwrap(), Timeframe::H4);
        assert!(matches!(
            Timeframe::from_str("2m"),
            Err(CoreError::InvalidTimeframe(_))
        ));
    }
}
