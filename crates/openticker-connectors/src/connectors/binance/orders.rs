use super::de::{
    deserialize_f64_from_string_or_number, deserialize_option_f64_from_string_or_number,
};
use super::http::{decode_order_submission_json, sign_query};
use crate::{ConnectorError, ConnectorKind, unix_now_ms};
use openticker_execution::{AcceptedOrder, OrderSide, OrderType};
use serde::Deserialize;

const BINANCE_MAX_QUANTITY_DECIMALS: usize = 8;
const BINANCE_MAX_QUANTITY_DECIMALS_I32: i32 = 8;
const BINANCE_QUANTITY_FLOOR_TOLERANCE: f64 = 1e-6;

#[derive(Debug, Deserialize)]
pub(super) struct BinanceSubmittedOrderPayload {
    pub(super) symbol: String,
    #[serde(rename = "clientOrderId")]
    pub(super) client_order_id: String,
    pub(super) status: String,
    #[serde(
        rename = "executedQty",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    pub(super) executed_qty: f64,
    #[serde(
        rename = "cummulativeQuoteQty",
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    pub(super) cumulative_quote_qty: Option<f64>,
    #[serde(default)]
    pub(super) fills: Vec<BinanceSubmittedOrderFillPayload>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinanceSubmittedOrderFillPayload {
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) price: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    pub(super) qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    pub(super) commission: Option<f64>,
    #[serde(rename = "commissionAsset", default)]
    pub(super) commission_asset: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn submit_binance_market_order(
    client: &reqwest::blocking::Client,
    base_url: &str,
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    side: &str,
    quantity: &str,
    client_order_id: &str,
) -> Result<BinanceSubmittedOrderPayload, ConnectorError> {
    let timestamp = unix_now_ms();
    let query = format!(
        "symbol={symbol}&side={side}&type=MARKET&quantity={quantity}&newClientOrderId={client_order_id}&newOrderRespType=FULL&recvWindow=5000&timestamp={timestamp}"
    );
    let signature = sign_query(api_secret, &query)?;
    let response = client
        .post(format!(
            "{base_url}/api/v3/order?{query}&signature={signature}"
        ))
        .header("X-MBX-APIKEY", api_key)
        .send()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("order submission request failed: {error}"),
        })?;
    decode_order_submission_json(response, "submit order")
}

pub(super) fn format_binance_quantity(value: f64) -> Result<String, ConnectorError> {
    if !value.is_finite() {
        return Err(ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("order quantity `{value}` is not a finite number"),
        });
    }
    if value <= 0.0 {
        return Ok("0".to_owned());
    }

    let scale = 10_f64.powi(BINANCE_MAX_QUANTITY_DECIMALS_I32);
    let floored = ((value * scale) + BINANCE_QUANTITY_FLOOR_TOLERANCE).floor() / scale;
    let precision = BINANCE_MAX_QUANTITY_DECIMALS;
    let mut formatted = format!("{floored:.precision$}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    Ok(formatted)
}

pub(super) fn fetch_binance_order_status(
    client: &reqwest::blocking::Client,
    base_url: &str,
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    client_order_id: &str,
) -> Result<BinanceSubmittedOrderPayload, ConnectorError> {
    let timestamp = unix_now_ms();
    let query = format!(
        "symbol={symbol}&origClientOrderId={client_order_id}&recvWindow=5000&timestamp={timestamp}"
    );
    let signature = sign_query(api_secret, &query)?;
    let response = client
        .get(format!(
            "{base_url}/api/v3/order?{query}&signature={signature}"
        ))
        .header("X-MBX-APIKEY", api_key)
        .send()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("order status request failed: {error}"),
        })?;
    decode_order_submission_json(response, "order status")
}

pub(super) fn accepted_order_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    side: OrderSide,
    fallback_price: f64,
) -> Option<AcceptedOrder> {
    let accepted_quantity = net_quantity_from_binance_payload(payload, side);
    if accepted_quantity <= f64::EPSILON {
        return None;
    }

    let weighted_fill_price =
        payload
            .fills
            .iter()
            .fold((0.0, 0.0), |(notional_sum, qty_sum), fill| {
                (notional_sum + (fill.price * fill.qty), qty_sum + fill.qty)
            });
    let fill_price = if weighted_fill_price.1 > f64::EPSILON {
        weighted_fill_price.0 / weighted_fill_price.1
    } else if let Some(cumulative_quote_qty) = payload.cumulative_quote_qty {
        if cumulative_quote_qty > f64::EPSILON {
            cumulative_quote_qty / payload.executed_qty
        } else {
            fallback_price.max(0.0)
        }
    } else {
        fallback_price.max(0.0)
    };
    let (fee_asset, fee_amount, fee_normalized_usd) =
        fee_details_from_binance_payload(payload, fill_price);

    Some(AcceptedOrder {
        client_order_id: payload.client_order_id.clone(),
        side,
        order_type: OrderType::Market,
        price: fill_price,
        quantity: accepted_quantity,
        fee_asset,
        fee_amount,
        fee_normalized_usd,
    })
}

fn fee_details_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    fill_price: f64,
) -> (Option<String>, Option<f64>, Option<f64>) {
    let mut fee_asset = None;
    let mut fee_amount = 0.0;
    let mut mixed_assets = false;
    let mut fee_normalized_usd = 0.0;
    let mut has_normalized_fee = false;
    let quote_asset = usd_quote_asset_from_symbol(payload.symbol.as_str());
    let base_asset = quote_asset.and_then(|quote_asset| payload.symbol.strip_suffix(quote_asset));

    for fill in &payload.fills {
        let Some(commission_asset) = fill.commission_asset.as_ref() else {
            continue;
        };
        let Some(commission) = fill
            .commission
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            continue;
        };

        match fee_asset.as_deref() {
            None => fee_asset = Some(commission_asset.clone()),
            Some(existing_asset) if existing_asset == commission_asset => {}
            Some(_) => mixed_assets = true,
        }
        fee_amount += commission;

        let normalized_fee =
            if quote_asset.is_some_and(|asset| asset.eq_ignore_ascii_case(commission_asset)) {
                Some(commission)
            } else if base_asset.is_some_and(|asset| asset.eq_ignore_ascii_case(commission_asset))
                && fill_price.is_finite()
                && fill_price > f64::EPSILON
            {
                Some(commission * fill_price)
            } else {
                None
            };

        if let Some(normalized_fee) = normalized_fee {
            fee_normalized_usd += normalized_fee;
            has_normalized_fee = true;
        }
    }

    (
        if mixed_assets { None } else { fee_asset },
        if !mixed_assets && fee_amount > f64::EPSILON {
            Some(fee_amount)
        } else {
            None
        },
        if has_normalized_fee {
            Some(fee_normalized_usd)
        } else {
            None
        },
    )
}

fn usd_quote_asset_from_symbol(symbol: &str) -> Option<&'static str> {
    ["USDT", "USDC", "FDUSD", "BUSD", "USD"]
        .into_iter()
        .find(|quote_asset| symbol.ends_with(quote_asset))
}

fn net_quantity_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    side: OrderSide,
) -> f64 {
    if !matches!(side, OrderSide::Buy) {
        return payload.executed_qty.max(0.0);
    }

    let base_asset_commission = payload
        .fills
        .iter()
        .filter_map(|fill| {
            let commission_asset = fill.commission_asset.as_deref()?;
            let base_asset = payload.symbol.get(..commission_asset.len())?;
            if base_asset.eq_ignore_ascii_case(commission_asset) {
                fill.commission
            } else {
                None
            }
        })
        .sum::<f64>();

    (payload.executed_qty - base_asset_commission).max(0.0)
}

pub(super) fn binance_order_status_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "FILLED" | "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
    )
}

pub(super) fn binance_order_side_label(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "BUY",
        OrderSide::Sell => "SELL",
    }
}
