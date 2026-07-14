use super::AccountLedger;
use crate::{LedgerError, LedgerException, LedgerExceptionKind, LedgerOwnerPath, ReservationError};

#[test]
fn effective_cap_uses_declared_total_until_live_balance_is_lower() {
    let mut ledger = AccountLedger::new(1_000.0);

    assert!((ledger.effective_cap_usd() - 1_000.0).abs() < 1e-6);

    ledger.set_live_balance_usd(Some(1_200.0));
    assert!((ledger.effective_cap_usd() - 1_000.0).abs() < 1e-6);

    ledger.set_live_balance_usd(Some(800.0));
    assert!((ledger.effective_cap_usd() - 800.0).abs() < 1e-6);
}

#[test]
fn partial_open_fill_releases_unused_reservation() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);

    assert!(ledger.try_reserve_open(&owner, 400.0, 50.0).is_ok());
    assert!((ledger.total_reserved_open_notional_usd() - 400.0).abs() < 1e-6);
    assert!(ledger.total_attributed_open_notional_usd().abs() < 1e-6);

    ledger.reconcile_open_fill(&owner, 250.0, 400.0);
    assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
    assert!((ledger.total_attributed_open_notional_usd() - 250.0).abs() < 1e-6);
    assert!((ledger.total_committed_notional_usd() - 250.0).abs() < 1e-6);
}

#[test]
fn fill_larger_than_reserved_does_not_double_count_committed_notional() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);

    assert!(ledger.try_reserve_open(&owner, 200.0, 100.0).is_ok());
    ledger.reconcile_open_fill(&owner, 300.0, 200.0);

    assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
    assert!((ledger.total_attributed_open_notional_usd() - 300.0).abs() < 1e-6);
    assert!((ledger.total_committed_notional_usd() - 300.0).abs() < 1e-6);
}

#[test]
fn blocking_exception_zeroes_tradeable_room_without_corrupting_available_room() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);
    ledger.replace_lane_open_notional([(owner, 250.0)]);
    ledger.replace_exceptions(vec![LedgerException {
        kind: LedgerExceptionKind::AmbiguousSymbolOwner,
        owner: None,
        symbol: Some("MSFT".to_owned()),
        detail: "matching_bots=bot-a,bot-b".to_owned(),
        blocks_new_opens: true,
    }]);

    let snapshot = ledger.account_snapshot("acct");
    assert!((ledger.account_available_open_room_usd() - 750.0).abs() < 1e-6);
    assert!((snapshot.attributed_open_notional_usd - 250.0).abs() < 1e-6);
    assert!(snapshot.unattributed_open_notional_usd.abs() < 1e-6);
    assert!(snapshot.tradeable_open_room_usd.abs() < 1e-6);
    assert!((snapshot.blocked_open_room_usd - 750.0).abs() < 1e-6);
}

#[test]
fn lane_snapshots_are_sorted_and_skip_zero_committed_rows() {
    let owner_a = LedgerOwnerPath::new("acct", "bot-b", "MSFT");
    let owner_b = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let owner_c = LedgerOwnerPath::new("acct", "bot-c", "NVDA");
    let mut ledger = AccountLedger::new(1_000.0);
    ledger.replace_lane_open_notional([(owner_a.clone(), 100.0), (owner_b.clone(), 50.0)]);
    assert!(ledger.try_reserve_open(&owner_c, 25.0, 100.0).is_ok());
    assert!(ledger.release_reservation(&owner_c, 25.0).is_ok());

    let snapshots = ledger.lane_snapshots();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].owner, owner_b);
    assert_eq!(snapshots[1].owner, owner_a);
}

#[test]
fn reservation_release_is_idempotent_under_small_dust_values() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);

    assert!(ledger.try_reserve_open(&owner, 100.0, 100.0).is_ok());
    assert!(ledger.release_reservation(&owner, 100.0 - 5e-10).is_ok());
    assert!(ledger.release_reservation(&owner, 1.0).is_ok());

    assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
    assert!(ledger.lane_snapshots().is_empty());
}

#[test]
fn bot_and_account_caps_both_apply_to_reservations() {
    let bot_a = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let bot_b = LedgerOwnerPath::new("acct", "bot-b", "MSFT");
    let mut ledger = AccountLedger::new(1_000.0);

    assert!(ledger.try_reserve_open(&bot_a, 400.0, 40.0).is_ok());
    assert_eq!(
        ledger.try_reserve_open(&bot_a, 10.0, 40.0),
        Err(ReservationError::BotCapacityExceeded)
    );

    assert!(ledger.try_reserve_open(&bot_b, 500.0, 60.0).is_ok());
    assert_eq!(
        ledger.try_reserve_open(&LedgerOwnerPath::new("acct", "bot-c", "NVDA"), 200.0, 30.0,),
        Err(ReservationError::AccountCapacityExceeded)
    );
}

#[test]
fn release_reservation_rejects_non_positive_and_non_finite_amounts() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);
    assert!(ledger.try_reserve_open(&owner, 400.0, 100.0).is_ok());

    for invalid in [-100.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            ledger.release_reservation(&owner, invalid),
            Err(LedgerError::InvalidReleaseAmount),
            "expected rejection for release amount {invalid}"
        );
    }

    // Rejected releases must leave the reservation untouched (no silent
    // no-op that traps notional, and no accidental mutation).
    assert!((ledger.total_reserved_open_notional_usd() - 400.0).abs() < 1e-6);
}

#[test]
fn release_position_rejects_non_positive_and_non_finite_amounts() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);
    ledger.replace_lane_open_notional([(owner.clone(), 300.0)]);

    for invalid in [-50.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            ledger.release_position(&owner, invalid),
            Err(LedgerError::InvalidReleaseAmount),
            "expected rejection for release amount {invalid}"
        );
    }

    assert!((ledger.total_attributed_open_notional_usd() - 300.0).abs() < 1e-6);
}

#[test]
fn valid_positive_release_succeeds_and_decrements() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);
    assert!(ledger.try_reserve_open(&owner, 400.0, 100.0).is_ok());

    assert!(ledger.release_reservation(&owner, 150.0).is_ok());
    assert!((ledger.total_reserved_open_notional_usd() - 250.0).abs() < 1e-6);

    ledger.replace_lane_open_notional([(owner.clone(), 300.0)]);
    assert!(ledger.release_position(&owner, 120.0).is_ok());
    assert!((ledger.total_attributed_open_notional_usd() - 180.0).abs() < 1e-6);
}

#[test]
fn over_release_clamps_at_zero_floor_and_still_returns_ok() {
    let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
    let mut ledger = AccountLedger::new(1_000.0);
    assert!(ledger.try_reserve_open(&owner, 100.0, 100.0).is_ok());

    // Releasing more than is reserved is a valid positive request: it must
    // succeed and clamp the tracked total at zero (never go negative).
    assert!(ledger.release_reservation(&owner, 500.0).is_ok());
    assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);

    ledger.replace_lane_open_notional([(owner.clone(), 80.0)]);
    assert!(ledger.release_position(&owner, 500.0).is_ok());
    assert!(ledger.total_attributed_open_notional_usd().abs() < 1e-6);
}
