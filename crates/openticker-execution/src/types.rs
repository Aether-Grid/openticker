use chrono::{DateTime, Utc};
use openticker_core::TradeIntent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub instance_id: String,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub intent: TradeIntent,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptedOrder {
    pub client_order_id: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
    pub fee_asset: Option<String>,
    pub fee_amount: Option<f64>,
    pub fee_normalized_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderLedgerOutcome {
    BotExhausted,
    AccountExhausted,
    Dust,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderQuantityResolution {
    pub quantity: f64,
    pub adjustment_reason: Option<String>,
    pub ledger_outcome: Option<OrderLedgerOutcome>,
}
