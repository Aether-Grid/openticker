pub(crate) const LEDGER_VALUE_TOLERANCE: f64 = 1e-9;

pub(crate) fn sanitize_value(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[must_use]
pub fn sanitize_ledger_value(value: f64) -> f64 {
    sanitize_value(value)
}

#[must_use]
pub fn calculate_position_notional_usd(quantity: f64, price_usd: f64) -> f64 {
    if quantity <= LEDGER_VALUE_TOLERANCE || !quantity.is_finite() || !price_usd.is_finite() {
        0.0
    } else {
        quantity * price_usd
    }
}
