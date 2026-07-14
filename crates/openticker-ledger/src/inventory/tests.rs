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

#[test]
fn large_pnl_accumulation_stays_finite() {
    // Realistic-extreme scenario: a very large position repeatedly sold in
    // chunks at a steep markup. This validates that the realized-P&L
    // accumulators stay finite and arithmetically correct across many
    // iterations. At these magnitudes (~1e18, ~290 orders of magnitude below
    // f64::MAX) true f64 overflow to +/-Inf is not reachable, so the
    // finiteness debug_assert is defense-in-depth rather than an exercised
    // failure path.
    let mut inventory = InventoryState::default();
    let huge_quantity = 1.0e12;
    let entry_price = 1.0e6;
    inventory
        .apply_fill(InventoryFillSide::Buy, huge_quantity, entry_price, None)
        .unwrap();

    let chunk = huge_quantity / 1_000.0;
    let exit_price = entry_price * 2.0;
    for _ in 0..1_000 {
        inventory
            .apply_fill(
                InventoryFillSide::Sell,
                chunk,
                exit_price,
                Some(&FeeEntry {
                    asset: "USD".to_owned(),
                    amount: 1.0e3,
                    normalized_usd: Some(1.0e3),
                }),
            )
            .unwrap();
    }

    assert!(inventory.realized_pnl.gross_usd.is_finite());
    assert!(inventory.realized_pnl.fees_usd.is_finite());
    assert!(inventory.realized_pnl.net_usd.is_finite());
    assert!(inventory.realized_pnl.gross_usd > 0.0);
    // gross = qty * (exit - entry) = 1e12 * 1e6 = 1e18; net subtracts fees.
    assert!((inventory.realized_pnl.gross_usd - 1.0e18).abs() / 1.0e18 < 1e-9);
}
