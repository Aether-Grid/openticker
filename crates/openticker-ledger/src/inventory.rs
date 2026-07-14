use serde::Serialize;

use crate::util::{LEDGER_VALUE_TOLERANCE, calculate_position_notional_usd, sanitize_value};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionLot {
    pub quantity: f64,
    pub average_cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeeEntry {
    pub asset: String,
    pub amount: f64,
    pub normalized_usd: Option<f64>,
}

impl FeeEntry {
    #[must_use]
    pub fn normalized_usd_or_zero(&self) -> f64 {
        self.normalized_usd.map_or(0.0, sanitize_value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryFillSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryError {
    InvalidQuantity,
    InvalidPrice,
    InsufficientQuantity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RealizedPnl {
    pub gross_usd: f64,
    pub fees_usd: f64,
    pub net_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnrealizedPnl {
    pub market_value_usd: f64,
    pub gross_usd: f64,
    pub fees_usd: f64,
    pub net_usd: f64,
    pub stale_mark: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValuationMark {
    pub symbol: String,
    pub price_usd: Option<f64>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct InventoryState {
    pub lot: Option<PositionLot>,
    pub realized_pnl: RealizedPnl,
}

impl InventoryState {
    #[must_use]
    pub fn from_position_state(
        position_quantity: f64,
        entry_price: Option<f64>,
        realized_pnl_usd: f64,
    ) -> Self {
        let position_quantity = sanitize_value(position_quantity);
        let entry_price = entry_price.filter(|price| price.is_finite() && *price > 0.0);
        let realized_pnl_usd = if realized_pnl_usd.is_finite() {
            realized_pnl_usd
        } else {
            0.0
        };

        Self {
            lot: if position_quantity > LEDGER_VALUE_TOLERANCE {
                entry_price.map(|average_cost_usd| PositionLot {
                    quantity: position_quantity,
                    average_cost_usd,
                })
            } else {
                None
            },
            realized_pnl: RealizedPnl {
                gross_usd: realized_pnl_usd,
                fees_usd: 0.0,
                net_usd: realized_pnl_usd,
            },
        }
    }

    #[must_use]
    pub fn quantity(&self) -> f64 {
        self.lot.as_ref().map_or(0.0, |lot| lot.quantity)
    }

    #[must_use]
    pub fn average_cost_usd(&self) -> Option<f64> {
        self.lot.as_ref().map(|lot| lot.average_cost_usd)
    }

    #[must_use]
    pub fn position_notional_usd(&self, valuation_price: Option<f64>) -> f64 {
        let Some(price_usd) = valuation_price.map(sanitize_value) else {
            return 0.0;
        };
        if price_usd <= LEDGER_VALUE_TOLERANCE {
            return 0.0;
        }

        calculate_position_notional_usd(self.quantity(), price_usd)
    }

    /// Applies a buy or sell fill to the in-memory inventory lot.
    ///
    /// # Errors
    ///
    /// Returns `InventoryError::InvalidQuantity` for non-positive quantity,
    /// `InventoryError::InvalidPrice` for non-positive price, and
    /// `InventoryError::InsufficientQuantity` when a sell exceeds held
    /// quantity.
    pub fn apply_fill(
        &mut self,
        side: InventoryFillSide,
        quantity: f64,
        price_usd: f64,
        fee: Option<&FeeEntry>,
    ) -> Result<(), InventoryError> {
        let quantity = sanitize_value(quantity);
        if quantity <= LEDGER_VALUE_TOLERANCE {
            return Err(InventoryError::InvalidQuantity);
        }

        let price_usd = sanitize_value(price_usd);
        if price_usd <= LEDGER_VALUE_TOLERANCE {
            return Err(InventoryError::InvalidPrice);
        }

        let fee_usd = fee.map_or(0.0, FeeEntry::normalized_usd_or_zero);
        match side {
            InventoryFillSide::Buy => {
                let previous_quantity = self.quantity();
                let previous_cost_usd = self
                    .lot
                    .as_ref()
                    .map_or(0.0, |lot| lot.quantity * lot.average_cost_usd);
                let next_quantity = previous_quantity + quantity;
                let next_cost_usd = previous_cost_usd + (quantity * price_usd) + fee_usd;
                self.lot = Some(PositionLot {
                    quantity: next_quantity,
                    average_cost_usd: next_cost_usd / next_quantity,
                });
            }
            InventoryFillSide::Sell => {
                let Some(lot) = self.lot.as_ref() else {
                    return Err(InventoryError::InsufficientQuantity);
                };
                if quantity > lot.quantity + LEDGER_VALUE_TOLERANCE {
                    return Err(InventoryError::InsufficientQuantity);
                }

                let gross_pnl_usd = (price_usd - lot.average_cost_usd) * quantity;
                self.realized_pnl.gross_usd += gross_pnl_usd;
                self.realized_pnl.fees_usd += fee_usd;
                self.realized_pnl.net_usd += gross_pnl_usd - fee_usd;
                // Accounting invariant: realized P&L accumulators must stay
                // finite. f64 overflow to +/-Inf would corrupt all downstream
                // accounting, so assert finiteness rather than silently
                // clamping (which would itself corrupt the books).
                debug_assert!(
                    self.realized_pnl.gross_usd.is_finite(),
                    "realized gross P&L overflowed to a non-finite value"
                );
                debug_assert!(
                    self.realized_pnl.fees_usd.is_finite(),
                    "realized fees overflowed to a non-finite value"
                );
                debug_assert!(
                    self.realized_pnl.net_usd.is_finite(),
                    "realized net P&L overflowed to a non-finite value"
                );

                let remaining_quantity = (lot.quantity - quantity).max(0.0);
                if remaining_quantity <= LEDGER_VALUE_TOLERANCE {
                    self.lot = None;
                } else {
                    self.lot = Some(PositionLot {
                        quantity: remaining_quantity,
                        average_cost_usd: lot.average_cost_usd,
                    });
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn unrealized_pnl(&self, mark: &ValuationMark) -> Option<UnrealizedPnl> {
        let lot = self.lot.as_ref()?;
        let price_usd = mark.price_usd.map(sanitize_value)?;
        if price_usd <= LEDGER_VALUE_TOLERANCE {
            return None;
        }

        let market_value_usd = lot.quantity * price_usd;
        let gross_usd = (price_usd - lot.average_cost_usd) * lot.quantity;
        Some(UnrealizedPnl {
            market_value_usd,
            gross_usd,
            fees_usd: 0.0,
            net_usd: gross_usd,
            stale_mark: mark.stale,
        })
    }
}

#[cfg(test)]
mod tests;
