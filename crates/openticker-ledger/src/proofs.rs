use crate::{AccountLedger, InventoryError, InventoryFillSide, InventoryState, LedgerOwnerPath};

fn quantity(units: u8) -> f64 {
    f64::from((units % 10) + 1)
}

fn price(units: u8) -> f64 {
    f64::from((units % 20) + 1)
}

fn owner() -> LedgerOwnerPath {
    LedgerOwnerPath::new("acct-1", "bot-a", "AAPL")
}

#[kani::proof]
fn proof_inventory_sell_cannot_go_negative() {
    let buy_quantity = quantity(kani::any());
    let sell_quantity = quantity(kani::any());
    kani::assume(sell_quantity <= buy_quantity);

    let mut inventory = InventoryState::default();
    assert!(
        inventory
            .apply_fill(
                InventoryFillSide::Buy,
                buy_quantity,
                price(kani::any()),
                None
            )
            .is_ok()
    );
    assert!(
        inventory
            .apply_fill(
                InventoryFillSide::Sell,
                sell_quantity,
                price(kani::any()),
                None
            )
            .is_ok()
    );

    assert!(inventory.quantity() >= 0.0);
    assert!(inventory.quantity().is_finite());
}

#[kani::proof]
fn proof_inventory_oversell_always_errors() {
    let buy_quantity = quantity(kani::any());
    let mut inventory = InventoryState::default();
    assert!(
        inventory
            .apply_fill(
                InventoryFillSide::Buy,
                buy_quantity,
                price(kani::any()),
                None
            )
            .is_ok()
    );

    let oversell = inventory.apply_fill(
        InventoryFillSide::Sell,
        buy_quantity + 1.0,
        price(kani::any()),
        None,
    );
    assert!(matches!(
        oversell,
        Err(InventoryError::InsufficientQuantity)
    ));
}

#[kani::proof]
fn proof_reservation_never_makes_tradeable_room_negative() {
    let mut ledger = AccountLedger::new(1_000.0);
    let owner = owner();
    let reserve_notional = f64::from((kani::any::<u8>() % 10) + 1) * 10.0;
    let before = ledger.account_tradeable_open_room_usd();

    assert!(
        ledger
            .try_reserve_open(&owner, reserve_notional, 50.0)
            .is_ok()
    );

    let after = ledger.account_tradeable_open_room_usd();
    assert!(after >= 0.0);
    assert!(after <= before);
}

#[kani::proof]
fn proof_release_cannot_make_committed_notional_negative() {
    let mut ledger = AccountLedger::new(1_000.0);
    let owner = owner();

    ledger.release_reservation(&owner, f64::from((kani::any::<u8>() % 20) + 1) * 10.0);
    ledger.release_position(&owner, f64::from((kani::any::<u8>() % 20) + 1) * 10.0);

    assert!(ledger.total_committed_notional_usd() >= 0.0);
    assert!(ledger.total_reserved_open_notional_usd() >= 0.0);
    assert!(ledger.total_attributed_open_notional_usd() >= 0.0);
}

#[kani::proof]
fn proof_reconcile_open_fill_keeps_totals_non_negative() {
    let mut ledger = AccountLedger::new(1_000.0);
    let owner = owner();
    assert!(ledger.try_reserve_open(&owner, 100.0, 50.0).is_ok());

    let filled_notional = f64::from((kani::any::<u8>() % 10) + 1) * 10.0;
    let reserved_notional = f64::from((kani::any::<u8>() % 10) + 1) * 10.0;
    ledger.reconcile_open_fill(&owner, filled_notional, reserved_notional);

    assert!(ledger.total_committed_notional_usd() >= 0.0);
    assert!(ledger.total_reserved_open_notional_usd() >= 0.0);
    assert!(ledger.total_attributed_open_notional_usd() >= 0.0);
}
