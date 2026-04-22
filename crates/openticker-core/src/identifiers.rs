use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::CoreError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(pub String);

impl InstanceId {
    /// Parses and validates an instance identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidIdentifier`] when `value` is blank after trimming.
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CoreError::InvalidIdentifier("instance id"));
        }
        Ok(Self(value))
    }
}

impl Display for InstanceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(pub String);

impl AccountId {
    /// Parses and validates an account identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidIdentifier`] when `value` is blank after trimming.
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CoreError::InvalidIdentifier("account id"));
        }
        Ok(Self(value))
    }
}

impl Display for AccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BotLaneKey {
    pub bot_id: String,
    pub symbol: String,
}

impl BotLaneKey {
    /// Parses and validates a bot-symbol runtime lane identity.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidIdentifier`] when either field is blank after trimming.
    pub fn parse(bot_id: impl Into<String>, symbol: impl Into<String>) -> Result<Self, CoreError> {
        let bot_id = bot_id.into();
        if bot_id.trim().is_empty() {
            return Err(CoreError::InvalidIdentifier("bot id"));
        }

        let symbol = symbol.into();
        if symbol.trim().is_empty() {
            return Err(CoreError::InvalidIdentifier("symbol"));
        }

        Ok(Self { bot_id, symbol })
    }

    #[must_use]
    pub fn encoded(&self) -> String {
        format!("{}::{}", self.bot_id, self.symbol)
    }
}

impl Display for BotLaneKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.encoded())
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountId, BotLaneKey, InstanceId};
    use crate::CoreError;

    #[test]
    fn validates_identifiers() {
        assert!(InstanceId::parse("alpha").is_ok());
        assert!(matches!(
            InstanceId::parse("  "),
            Err(CoreError::InvalidIdentifier(_))
        ));
        assert!(AccountId::parse("paper").is_ok());
        assert!(BotLaneKey::parse("swing", "AAPL").is_ok());
    }
}
