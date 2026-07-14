use super::de::deserialize_f64_from_string_or_number;
use crate::{
    ConnectorError, ConnectorKind, ConnectorOpenOrder, ConnectorPosition, ConnectorPrivateBalance,
    ConnectorSymbolConstraints, sanitize_symbol_for_error,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct BinanceOpenOrderPayload {
    #[serde(rename = "clientOrderId")]
    pub(super) client_order_id: String,
    pub(super) symbol: String,
    pub(super) status: String,
    #[serde(
        rename = "origQty",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    pub(super) orig_qty: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinanceAccountPayload {
    pub(super) balances: Vec<BinanceBalancePayload>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinanceExchangeInfoPayload {
    #[serde(default)]
    pub(super) symbols: Vec<BinanceExchangeSymbolPayload>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinanceExchangeSymbolPayload {
    pub(super) symbol: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) filters: Vec<BinanceExchangeFilterPayload>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinanceExchangeFilterPayload {
    #[serde(rename = "filterType")]
    pub(super) filter_type: String,
    #[serde(rename = "minQty", default)]
    pub(super) min_qty: Option<String>,
    #[serde(rename = "stepSize", default)]
    pub(super) step_size: Option<String>,
    #[serde(rename = "minNotional", default)]
    pub(super) min_notional: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct BinanceBalancePayload {
    pub(super) asset: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) free: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) locked: f64,
}

pub(super) fn extract_symbol_constraints(
    payload: BinanceExchangeInfoPayload,
    symbol: &str,
) -> Result<ConnectorSymbolConstraints, ConnectorError> {
    let symbol_payload = payload
        .symbols
        .into_iter()
        .find(|entry| entry.symbol == symbol)
        .ok_or_else(|| ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!(
                "exchangeInfo did not include symbol `{}`",
                sanitize_symbol_for_error(symbol)
            ),
        })?;

    if !symbol_payload.status.eq_ignore_ascii_case("TRADING") {
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!(
                "symbol `{}` is not trading (status={})",
                sanitize_symbol_for_error(symbol),
                symbol_payload.status
            ),
        });
    }

    let mut quantity_step = None;
    let mut min_quantity = None;
    let mut min_notional_usd: Option<f64> = None;

    for filter in symbol_payload.filters {
        match filter.filter_type.as_str() {
            "LOT_SIZE" => {
                if let Some(parsed_step) = parse_positive_decimal(filter.step_size.as_deref()) {
                    quantity_step = Some(parsed_step);
                }
                if let Some(parsed_min_qty) = parse_positive_decimal(filter.min_qty.as_deref()) {
                    min_quantity = Some(parsed_min_qty);
                }
            }
            "MIN_NOTIONAL" | "NOTIONAL" => {
                if let Some(parsed_notional) =
                    parse_positive_decimal(filter.min_notional.as_deref())
                {
                    min_notional_usd = Some(
                        min_notional_usd
                            .map_or(parsed_notional, |current| current.max(parsed_notional)),
                    );
                }
            }
            _ => {}
        }
    }

    Ok(ConnectorSymbolConstraints {
        fractional_entry_supported: None,
        quantity_step,
        min_quantity,
        min_notional_usd,
        source: Some("binance_exchange_info".to_owned()),
    })
}

fn parse_positive_decimal(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

pub(super) fn normalize_orders(payload: Vec<BinanceOpenOrderPayload>) -> Vec<ConnectorOpenOrder> {
    payload
        .into_iter()
        .map(|order| ConnectorOpenOrder {
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            status: order.status,
            quantity: order.orig_qty,
        })
        .collect()
}

pub(super) fn normalize_positions(payload: Vec<BinanceBalancePayload>) -> Vec<ConnectorPosition> {
    payload
        .into_iter()
        .filter_map(|balance| {
            let quantity = balance.free + balance.locked;
            if quantity.abs() <= f64::EPSILON || is_binance_cash_asset(balance.asset.as_str()) {
                None
            } else {
                Some(ConnectorPosition {
                    symbol: balance.asset,
                    quantity,
                })
            }
        })
        .collect()
}

fn is_binance_cash_asset(asset: &str) -> bool {
    matches!(asset, "USD" | "USDT" | "USDC" | "BUSD")
}

pub(super) fn normalize_balances(
    payload: Vec<BinanceBalancePayload>,
) -> Vec<ConnectorPrivateBalance> {
    payload
        .into_iter()
        .map(|balance| ConnectorPrivateBalance {
            asset: balance.asset,
            free: balance.free,
            locked: balance.locked,
        })
        .collect()
}
