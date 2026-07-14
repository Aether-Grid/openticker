use crate::POSITION_QUANTITY_TOLERANCE;
use openticker_connectors::ConnectorAccountSnapshot;
use openticker_ledger::sanitize_ledger_value;

const DEFAULT_BINANCE_CASH_BALANCE_ASSETS: [&str; 5] = ["USD", "USDT", "USDC", "BUSD", "FDUSD"];

#[must_use]
pub fn live_balance_from_snapshot(
    account_kind: &str,
    snapshot: &ConnectorAccountSnapshot,
    known_open_notional_usd: f64,
    cash_balance_assets: &[String],
) -> Option<f64> {
    match account_kind {
        "alpaca" => snapshot
            .balances
            .iter()
            .find(|balance| balance.asset.eq_ignore_ascii_case("EQUITY"))
            .map(|balance| sanitize_ledger_value(balance.free + balance.locked)),
        "binance" => {
            let quote_cash_usd = snapshot
                .balances
                .iter()
                .filter(|balance| {
                    is_binance_cash_balance_asset(balance.asset.as_str(), cash_balance_assets)
                })
                .map(|balance| sanitize_ledger_value(balance.free + balance.locked))
                .sum::<f64>();
            let computed = quote_cash_usd + known_open_notional_usd;
            if computed > POSITION_QUANTITY_TOLERANCE {
                Some(computed)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_binance_cash_balance_asset(asset: &str, cash_balance_assets: &[String]) -> bool {
    if cash_balance_assets.is_empty() {
        return DEFAULT_BINANCE_CASH_BALANCE_ASSETS
            .iter()
            .any(|configured_asset| asset.eq_ignore_ascii_case(configured_asset));
    }

    cash_balance_assets.iter().any(|configured_asset| {
        let configured_asset = configured_asset.trim();
        !configured_asset.is_empty() && asset.eq_ignore_ascii_case(configured_asset)
    })
}
