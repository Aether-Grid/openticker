use serde::Serialize;

use crate::calculate_position_notional_usd;
use crate::util::{LEDGER_VALUE_TOLERANCE, sanitize_value};

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
mod tests {
    use super::{FeeEntry, InventoryError, InventoryFillSide, InventoryState, ValuationMark};

    #[test]
    fn inventory_state_uses_weighted_average_cost_basis_with_buy_fees() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(
                InventoryFillSide::Buy,
                2.0,
                100.0,
                Some(&FeeEntry {
                    asset: "USD".to_owned(),
                    amount: 2.0,
                    normalized_usd: Some(2.0),
                }),
            )
            .unwrap();
        inventory
            .apply_fill(
                InventoryFillSide::Buy,
                1.0,
                130.0,
                Some(&FeeEntry {
                    asset: "USD".to_owned(),
                    amount: 1.0,
                    normalized_usd: Some(1.0),
                }),
            )
            .unwrap();

        assert!((inventory.quantity() - 3.0).abs() < 1e-6);
        assert!((inventory.average_cost_usd().unwrap() - 111.0).abs() < 1e-6);
    }

    #[test]
    fn inventory_state_reconstructs_from_position_state() {
        let inventory = InventoryState::from_position_state(2.5, Some(101.0), 12.0);

        assert!((inventory.quantity() - 2.5).abs() < 1e-6);
        assert!((inventory.average_cost_usd().unwrap() - 101.0).abs() < 1e-6);
        assert!((inventory.realized_pnl.net_usd - 12.0).abs() < 1e-6);
    }

    #[test]
    fn inventory_state_reports_position_notional_from_mark() {
        let inventory = InventoryState::from_position_state(2.0, Some(100.0), 0.0);

        assert!((inventory.position_notional_usd(Some(110.0)) - 220.0).abs() < 1e-6);
    }

    #[test]
    fn inventory_state_realizes_net_pnl_on_sell_with_fee() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(InventoryFillSide::Buy, 3.0, 100.0, None)
            .unwrap();
        inventory
            .apply_fill(
                InventoryFillSide::Sell,
                1.0,
                130.0,
                Some(&FeeEntry {
                    asset: "USD".to_owned(),
                    amount: 3.0,
                    normalized_usd: Some(3.0),
                }),
            )
            .unwrap();

        assert!((inventory.quantity() - 2.0).abs() < 1e-6);
        assert!((inventory.realized_pnl.gross_usd - 30.0).abs() < 1e-6);
        assert!((inventory.realized_pnl.fees_usd - 3.0).abs() < 1e-6);
        assert!((inventory.realized_pnl.net_usd - 27.0).abs() < 1e-6);
    }

    #[test]
    fn inventory_state_marks_unrealized_pnl_from_latest_mark() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(InventoryFillSide::Buy, 2.0, 100.0, None)
            .unwrap();

        let unrealized = inventory
            .unrealized_pnl(&ValuationMark {
                symbol: "AAPL".to_owned(),
                price_usd: Some(110.0),
                stale: false,
            })
            .unwrap();

        assert!((unrealized.market_value_usd - 220.0).abs() < 1e-6);
        assert!((unrealized.gross_usd - 20.0).abs() < 1e-6);
        assert!((unrealized.net_usd - 20.0).abs() < 1e-6);
        assert!(!unrealized.stale_mark);
    }

    #[test]
    fn inventory_state_rejects_sell_larger_than_inventory() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(InventoryFillSide::Buy, 1.0, 100.0, None)
            .unwrap();

        assert_eq!(
            inventory.apply_fill(InventoryFillSide::Sell, 2.0, 120.0, None),
            Err(InventoryError::InsufficientQuantity)
        );
        assert!((inventory.quantity() - 1.0).abs() < 1e-6);
        assert_eq!(inventory.average_cost_usd(), Some(100.0));
    }

    #[test]
    fn full_close_then_reopen_starts_new_cost_basis() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(InventoryFillSide::Buy, 2.0, 100.0, None)
            .unwrap();
        inventory
            .apply_fill(InventoryFillSide::Sell, 2.0, 120.0, None)
            .unwrap();
        inventory
            .apply_fill(InventoryFillSide::Buy, 1.0, 150.0, None)
            .unwrap();

        assert!((inventory.quantity() - 1.0).abs() < 1e-6);
        assert_eq!(inventory.average_cost_usd(), Some(150.0));
        assert!((inventory.realized_pnl.net_usd - 40.0).abs() < 1e-6);
    }

    #[test]
    fn invalid_fill_inputs_reject_without_mutation() {
        let mut inventory = InventoryState::default();
        inventory
            .apply_fill(InventoryFillSide::Buy, 1.0, 100.0, None)
            .unwrap();
        let baseline = inventory.clone();

        assert_eq!(
            inventory.apply_fill(InventoryFillSide::Buy, 0.0, 110.0, None),
            Err(InventoryError::InvalidQuantity)
        );
        assert_eq!(inventory, baseline);

        assert_eq!(
            inventory.apply_fill(InventoryFillSide::Sell, 1.0, 0.0, None),
            Err(InventoryError::InvalidPrice)
        );
        assert_eq!(inventory, baseline);
    }

    #[test]
    fn unrealized_pnl_requires_positive_mark() {
        let inventory = InventoryState::from_position_state(2.0, Some(100.0), 0.0);

        assert!(
            inventory
                .unrealized_pnl(&ValuationMark {
                    symbol: "AAPL".to_owned(),
                    price_usd: None,
                    stale: false,
                })
                .is_none()
        );
        assert!(
            inventory
                .unrealized_pnl(&ValuationMark {
                    symbol: "AAPL".to_owned(),
                    price_usd: Some(0.0),
                    stale: false,
                })
                .is_none()
        );
        assert!(
            inventory
                .unrealized_pnl(&ValuationMark {
                    symbol: "AAPL".to_owned(),
                    price_usd: Some(-1.0),
                    stale: false,
                })
                .is_none()
        );
    }
}
