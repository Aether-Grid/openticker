use openticker_connectors::{ConnectionState, ConnectorAccount, ConnectorKind, ConnectorRegistry};
use openticker_core::{ExecutionMode, Timeframe};
use openticker_gateway::{Gateway, GatewayError};
use std::sync::{Arc, Mutex};

fn test_account(
    account_id: &str,
    kind: ConnectorKind,
    mode: ExecutionMode,
    use_demo_mode: bool,
) -> ConnectorAccount {
    ConnectorAccount {
        account_id: account_id.to_owned(),
        kind,
        mode,
        use_demo_mode,
        api_key_env: None,
        api_secret_env: None,
        passphrase_env: None,
        reconciliation_remote_snapshot: false,
        execution_remote_submission: false,
        reconciliation_base_url: None,
    }
}

fn gateway_for_accounts(accounts: Vec<ConnectorAccount>) -> Gateway {
    let registry =
        ConnectorRegistry::from_accounts(accounts).expect("test connector registry should build");
    Gateway::new(Arc::new(Mutex::new(registry)))
}

#[test]
fn latest_bar_requires_ready_account() {
    let gateway = gateway_for_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )]);

    assert!(matches!(
        gateway.fetch_latest_bar("binance-paper", "BTCUSDT", Timeframe::M1),
        Err(GatewayError::ConnectorNotReady {
            account_id,
            state,
            reason
        }) if account_id == "binance-paper"
            && matches!(state, ConnectionState::Degraded)
            && reason.contains("consecutive_failures=1")
    ));
}

#[test]
fn account_snapshot_unchecked_bypasses_readiness_gate() {
    let gateway = gateway_for_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )]);

    let snapshot = gateway
        .fetch_account_snapshot_unchecked("binance-paper")
        .expect("unchecked snapshot should bypass readiness");
    assert!(snapshot.open_orders.is_empty());
    assert!(snapshot.positions.is_empty());
}

#[test]
fn unknown_account_returns_explicit_error() {
    let gateway = gateway_for_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )]);

    assert!(matches!(
        gateway.fetch_latest_bar("missing-account", "AAPL", Timeframe::M1),
        Err(GatewayError::UnknownAccount { account_id }) if account_id == "missing-account"
    ));

    assert!(matches!(
        gateway.fetch_account_snapshot_unchecked("missing-account"),
        Err(GatewayError::UnknownAccount { account_id }) if account_id == "missing-account"
    ));
}

#[test]
fn ensure_account_ready_records_disconnect_and_reconnect_bookkeeping() {
    let registry = Arc::new(Mutex::new(
        ConnectorRegistry::from_accounts(vec![test_account(
            "alpaca-paper",
            ConnectorKind::Alpaca,
            ExecutionMode::Paper,
            false,
        )])
        .expect("test connector registry should build"),
    ));
    let gateway = Gateway::new(Arc::clone(&registry));

    {
        let guard = registry.lock().expect("registry lock should succeed");
        guard
            .note_account_disconnect("alpaca-paper", 1_000)
            .expect("disconnect bookkeeping should succeed");
    }

    gateway
        .ensure_account_ready("alpaca-paper")
        .expect("connected account should pass readiness check");

    let status = gateway
        .statuses()
        .expect("status lookup should succeed")
        .into_iter()
        .find(|status| status.account_id == "alpaca-paper")
        .expect("alpaca-paper status should exist");
    assert_eq!(status.resilience_state.consecutive_failures, 0);
    assert!(status.resilience_state.next_reconnect_at_ms.is_none());
}

#[test]
fn ensure_account_ready_disconnect_bookkeeping_reflects_fresh_state() {
    // Covers the disconnect-bookkeeping path in `ensure_account_ready`: a
    // not-ready account must record the disconnect and surface a reason derived
    // from the *freshly recorded* resilience state, never a stale snapshot. The
    // bookkeeping call is best-effort (its failure is logged, not propagated),
    // so this asserts the success path keeps the reported state consistent.
    let registry = Arc::new(Mutex::new(
        ConnectorRegistry::from_accounts(vec![test_account(
            "binance-paper",
            ConnectorKind::Binance,
            ExecutionMode::Paper,
            false,
        )])
        .expect("test connector registry should build"),
    ));
    let gateway = Gateway::new(Arc::clone(&registry));

    // First readiness check on a never-connected account records one failure.
    let first = gateway.ensure_account_ready("binance-paper");
    assert!(matches!(
        first,
        Err(GatewayError::ConnectorNotReady {
            ref account_id,
            state,
            ref reason,
        }) if account_id == "binance-paper"
            && matches!(state, ConnectionState::Degraded)
            && reason.contains("consecutive_failures=1")
    ));

    // A second check records another disconnect; the reason must reflect the
    // updated count (=2), proving the post-bookkeeping status read is fresh and
    // not the stale value captured before the disconnect was recorded.
    let second = gateway.ensure_account_ready("binance-paper");
    assert!(matches!(
        second,
        Err(GatewayError::ConnectorNotReady { ref reason, .. })
            if reason.contains("consecutive_failures=2")
    ));

    let status = gateway
        .statuses()
        .expect("status lookup should succeed")
        .into_iter()
        .find(|status| status.account_id == "binance-paper")
        .expect("binance-paper status should exist");
    assert_eq!(status.resilience_state.consecutive_failures, 2);
}
