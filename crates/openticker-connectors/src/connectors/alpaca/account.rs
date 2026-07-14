use super::de::deserialize_f64_from_string_or_number;
use crate::{
    ConnectorOpenOrder, ConnectorPosition, ConnectorPrivateBalance, ConnectorSymbolConstraints,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaOrderPayload {
    pub(super) client_order_id: String,
    pub(super) symbol: String,
    pub(super) status: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) qty: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaPositionPayload {
    pub(super) symbol: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) qty: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaAccountPayload {
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) cash: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) equity: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct AlpacaAssetPayload {
    pub(super) tradable: bool,
    #[serde(default)]
    pub(super) fractionable: bool,
}

pub(super) fn symbol_constraints_from_asset(
    asset: &AlpacaAssetPayload,
) -> ConnectorSymbolConstraints {
    let (quantity_step, min_quantity) = if asset.fractionable {
        (None, None)
    } else {
        (Some(1.0), Some(1.0))
    };

    ConnectorSymbolConstraints {
        fractional_entry_supported: Some(asset.fractionable),
        quantity_step,
        min_quantity,
        min_notional_usd: None,
        source: Some(format!(
            "alpaca_asset_metadata(fractionable={})",
            asset.fractionable
        )),
    }
}

pub(super) fn normalize_orders(payload: Vec<AlpacaOrderPayload>) -> Vec<ConnectorOpenOrder> {
    payload
        .into_iter()
        .map(|order| ConnectorOpenOrder {
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            status: order.status,
            quantity: order.qty,
        })
        .collect()
}

pub(super) fn normalize_positions(payload: Vec<AlpacaPositionPayload>) -> Vec<ConnectorPosition> {
    payload
        .into_iter()
        .map(|position| ConnectorPosition {
            symbol: position.symbol,
            quantity: position.qty,
        })
        .collect()
}

pub(super) fn normalize_account_balances(
    payload: &AlpacaAccountPayload,
) -> Vec<ConnectorPrivateBalance> {
    vec![
        ConnectorPrivateBalance {
            asset: "CASH".to_owned(),
            free: payload.cash,
            locked: 0.0,
        },
        ConnectorPrivateBalance {
            asset: "EQUITY".to_owned(),
            free: payload.equity,
            locked: 0.0,
        },
    ]
}
