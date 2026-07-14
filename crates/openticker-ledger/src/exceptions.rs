use serde::Serialize;

use crate::ownership::LedgerOwnerPath;

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
