use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct LedgerOwnerPath {
    pub account_id: String,
    pub bot_id: String,
    pub symbol: String,
}

impl LedgerOwnerPath {
    #[must_use]
    pub fn new(
        account_id: impl Into<String>,
        bot_id: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            bot_id: bot_id.into(),
            symbol: symbol.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipPolicy {
    ExclusiveLanePerSymbol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipResolution {
    Owned(LedgerOwnerPath),
    Unmatched {
        account_id: String,
        symbol: String,
    },
    Ambiguous {
        account_id: String,
        symbol: String,
        bot_ids: Vec<String>,
    },
}
