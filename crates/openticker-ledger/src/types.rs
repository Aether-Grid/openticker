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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerExceptionKind {
    UnmatchedConnectorPosition,
    AmbiguousSymbolOwner,
    ManagedPositionDeficit,
    UnmappedManagedOpenOrder,
    UnpricedInventory,
    FeeNormalizationMissing,
}

impl LedgerExceptionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnmatchedConnectorPosition => "unmatched_connector_position",
            Self::AmbiguousSymbolOwner => "ambiguous_symbol_owner",
            Self::ManagedPositionDeficit => "managed_position_deficit",
            Self::UnmappedManagedOpenOrder => "unmapped_managed_open_order",
            Self::UnpricedInventory => "unpriced_inventory",
            Self::FeeNormalizationMissing => "fee_normalization_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerException {
    pub kind: LedgerExceptionKind,
    pub owner: Option<LedgerOwnerPath>,
    pub symbol: Option<String>,
    pub detail: String,
    pub blocks_new_opens: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BotAllocationPolicy {
    pub account_id: String,
    pub bot_id: String,
    pub pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationError {
    BotCapacityExceeded,
    AccountCapacityExceeded,
}

/// Error returned by mutating ledger accounting operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerError {
    /// The supplied release amount was not a finite, strictly positive USD
    /// value. Releases must subtract a concrete amount, so non-finite
    /// (`NaN`/`Inf`) and non-positive (`<= 0`) inputs are rejected instead of
    /// being silently sanitized to zero (which would trap reserved notional).
    InvalidReleaseAmount,
}
