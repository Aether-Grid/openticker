use openticker_connectors::{
    ConnectionState, ConnectorAccount, ConnectorError, ConnectorKind, ConnectorRegistry,
};
use openticker_core::ExecutionMode;

#[test]
fn checked_snapshot_path_fails_closed_when_account_degraded() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .expect("registry should build");

    assert!(matches!(
        registry.fetch_account_snapshot("binance-paper"),
        Err(ConnectorError::Unavailable { kind, state })
            if kind == ConnectorKind::Binance && matches!(state, ConnectionState::Degraded)
    ));
}

#[test]
fn unchecked_snapshot_path_bypasses_health_gate() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .expect("registry should build");

    let snapshot = registry
        .fetch_account_snapshot_unchecked("binance-paper")
        .expect("unchecked snapshot should bypass health gating");
    assert!(snapshot.open_orders.is_empty());
    assert!(snapshot.positions.is_empty());
}

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
