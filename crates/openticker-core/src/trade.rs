use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeIntent {
    NoOp,
    OpenLong,
    AddLong,
    ReduceLong,
    CloseLong,
}
