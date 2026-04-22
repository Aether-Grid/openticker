use chrono::{DateTime, Utc};
use openticker_core::{OhlcvBar, SignalPhase};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedTrade {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedQuote {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub bid: f64,
    pub ask: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedOrderEvent {
    pub symbol: String,
    pub client_order_id: String,
    pub status: String,
    pub side: String,
    pub order_quantity: f64,
    pub cumulative_filled_quantity: f64,
    pub last_fill_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedBarUpdate {
    pub symbol: String,
    pub phase: SignalPhase,
    pub bar: OhlcvBar,
}
