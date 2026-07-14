use super::de::deserialize_option_f64_from_string_or_number;
use openticker_execution::{AcceptedOrder, OrderSide, OrderType};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaSubmittedOrderPayload {
    pub(super) id: String,
    pub(super) client_order_id: String,
    pub(super) status: String,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    pub(super) filled_qty: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    pub(super) filled_avg_price: Option<f64>,
}

pub(super) fn accepted_order_from_alpaca_payload(
    payload: &AlpacaSubmittedOrderPayload,
    side: OrderSide,
    fallback_price: f64,
) -> Option<AcceptedOrder> {
    let quantity = payload.filled_qty.unwrap_or(0.0);
    if quantity <= f64::EPSILON {
        return None;
    }

    let price = payload
        .filled_avg_price
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback_price.max(0.0));

    Some(AcceptedOrder {
        client_order_id: payload.client_order_id.clone(),
        side,
        order_type: OrderType::Market,
        price,
        quantity,
        fee_asset: None,
        fee_amount: None,
        fee_normalized_usd: None,
    })
}

pub(super) fn alpaca_order_side_label(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

pub(super) fn alpaca_order_status_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "filled"
            | "canceled"
            | "expired"
            | "rejected"
            | "suspended"
            | "stopped"
            | "done_for_day"
            | "calculated"
            | "replaced"
    )
}
