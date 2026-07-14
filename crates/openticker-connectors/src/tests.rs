use super::*;
use chrono::{DateTime, Utc};
use openticker_core::{ExecutionMode, Timeframe, TradeIntent};
use openticker_execution::ExecutionRequest;

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

#[test]
fn connector_matrix_matches_v1_role_expectations() {
    let matrix = connector_matrix();
    assert_eq!(matrix.len(), 2);
    assert!(matrix.iter().all(|descriptor| {
        descriptor.resilience.reconnect_base_backoff_ms
            <= descriptor.resilience.reconnect_max_backoff_ms
    }));

    let alpaca = descriptor_for(ConnectorKind::Alpaca);
    assert_eq!(alpaca.role, ConnectorRole::Dual);
    assert!(alpaca.supports_paper);
    assert!(alpaca.resilience.requires_ping_pong_keepalive);

    let binance = descriptor_for(ConnectorKind::Binance);
    assert!(binance.supports_demo);
    assert!(binance.supports_live);
    assert_eq!(
        binance.resilience.max_connection_lifetime_secs,
        Some(86_400)
    );
    assert_eq!(binance.resilience.max_messages_per_second, Some(5));
}

#[test]
fn validates_mode_support_for_stub_connector() {
    let alpaca_paper = StubConnector::new(ConnectorKind::Alpaca, ExecutionMode::Paper, false);
    assert!(alpaca_paper.validate_mode().is_ok());

    let binance_without_demo =
        StubConnector::new(ConnectorKind::Binance, ExecutionMode::Paper, false);
    assert!(matches!(
        binance_without_demo.validate_mode(),
        Err(ConnectorError::DemoModeRequired { .. })
    ));

    let binance_with_demo = StubConnector::new(ConnectorKind::Binance, ExecutionMode::Paper, true);
    assert!(binance_with_demo.validate_mode().is_ok());
}

#[test]
fn parses_connector_kind_labels() {
    assert_eq!(ConnectorKind::parse("alpaca"), Some(ConnectorKind::Alpaca));
    assert_eq!(
        ConnectorKind::parse("binance"),
        Some(ConnectorKind::Binance)
    );
    assert_eq!(ConnectorKind::parse("unknown"), None);
    assert_eq!(ConnectorKind::Alpaca.as_str(), "alpaca");
}

#[test]
fn stub_connector_reconciliation_snapshot_respects_connection_state() {
    let mut connector = StubConnector::new(ConnectorKind::Alpaca, ExecutionMode::Paper, false);
    let snapshot = connector.fetch_account_snapshot().unwrap();
    assert!(snapshot.open_orders.is_empty());
    assert!(snapshot.positions.is_empty());

    connector.set_state(ConnectionState::Degraded);
    assert!(matches!(
        connector.fetch_account_snapshot(),
        Err(ConnectorError::Unavailable { .. })
    ));
}

#[test]
fn connector_registry_builds_account_statuses() {
    let registry = ConnectorRegistry::from_accounts(vec![
        test_account(
            "alpaca-paper",
            ConnectorKind::Alpaca,
            ExecutionMode::Paper,
            false,
        ),
        test_account(
            "binance-paper",
            ConnectorKind::Binance,
            ExecutionMode::Paper,
            false,
        ),
    ])
    .unwrap();

    let statuses = registry.statuses();
    assert_eq!(statuses.len(), 2);
    assert!(
        statuses
            .iter()
            .any(|status| status.account_id == "alpaca-paper"
                && matches!(status.state, ConnectionState::Connected))
    );
    assert!(statuses.iter().any(|status| {
        status.account_id == "alpaca-paper"
            && status.resilience_policy.reconnect_base_backoff_ms == 500
            && status.resilience_state.consecutive_failures == 0
    }));
    assert!(
        statuses
            .iter()
            .any(|status| status.account_id == "binance-paper"
                && matches!(status.state, ConnectionState::Degraded))
    );
    assert!(!registry.is_ready());
}

#[test]
fn connector_registry_rejects_duplicate_accounts() {
    let result = ConnectorRegistry::from_accounts(vec![
        test_account(
            "alpaca-paper",
            ConnectorKind::Alpaca,
            ExecutionMode::Paper,
            false,
        ),
        test_account(
            "alpaca-paper",
            ConnectorKind::Alpaca,
            ExecutionMode::Paper,
            false,
        ),
    ]);

    assert!(matches!(
        result,
        Err(ConnectorError::DuplicateAccountId { .. })
    ));
}

#[test]
fn connector_registry_fetches_snapshot_for_ready_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let snapshot = registry.fetch_account_snapshot("alpaca-paper").unwrap();
    assert!(snapshot.open_orders.is_empty());
    assert!(snapshot.positions.is_empty());
}

#[test]
fn connector_registry_fetches_latest_confirmed_timestamp_for_ready_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let timestamp = registry
        .fetch_latest_confirmed_bar_timestamp("alpaca-paper", "AAPL", Timeframe::M1)
        .expect("latest confirmed timestamp should load");
    assert!(timestamp.is_some());
}

#[test]
fn connector_registry_fetches_confirmed_range_page_oldest_first() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let end_at = registry
        .fetch_latest_confirmed_bar_timestamp("alpaca-paper", "AAPL", Timeframe::M1)
        .expect("latest confirmed timestamp should load")
        .expect("stub connector should provide a confirmed timestamp");
    let start_after = end_at - chrono::Duration::minutes(3);

    let page = registry
        .fetch_confirmed_bars_range(
            "alpaca-paper",
            "AAPL",
            Timeframe::M1,
            Some(start_after),
            end_at,
            10,
        )
        .expect("confirmed range page should load");

    assert!(!page.bars.is_empty());
    assert!(page.exhausted);
    assert!(
        page.bars
            .windows(2)
            .all(|window| window[0].timestamp < window[1].timestamp)
    );
    assert!(
        page.bars
            .first()
            .is_some_and(|bar| bar.timestamp > start_after)
    );
    assert!(page.bars.last().is_some_and(|bar| bar.timestamp <= end_at));
}

#[test]
fn connector_registry_tracks_disconnect_reconnect_and_rate_limit_windows() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    registry
        .note_account_disconnect("alpaca-paper", 1_000)
        .unwrap();
    let status_after_disconnect = registry.status_for_account("alpaca-paper").unwrap();
    assert_eq!(
        status_after_disconnect
            .resilience_state
            .consecutive_failures,
        1
    );
    assert!(
        status_after_disconnect
            .resilience_state
            .next_reconnect_at_ms
            .is_some()
    );

    registry
        .note_account_rate_limit("alpaca-paper", 2_000, 1_500)
        .unwrap();
    let status_after_rate_limit = registry.status_for_account("alpaca-paper").unwrap();
    assert_eq!(
        status_after_rate_limit.resilience_state.throttled_until_ms,
        Some(3_500)
    );

    registry
        .note_account_reconnect_success("alpaca-paper")
        .unwrap();
    let status_after_reconnect = registry.status_for_account("alpaca-paper").unwrap();
    assert_eq!(
        status_after_reconnect.resilience_state.consecutive_failures,
        0
    );
    assert!(
        status_after_reconnect
            .resilience_state
            .next_reconnect_at_ms
            .is_none()
    );
}

#[test]
fn connector_registry_blocks_snapshot_for_degraded_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    assert!(matches!(
        registry.fetch_account_snapshot("binance-paper"),
        Err(ConnectorError::Unavailable { .. })
    ));
}

#[test]
fn connector_registry_fetches_snapshot_unchecked_for_degraded_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let snapshot = registry
        .fetch_account_snapshot_unchecked("binance-paper")
        .unwrap();
    assert!(snapshot.open_orders.is_empty());
    assert!(snapshot.positions.is_empty());
}

#[test]
fn connector_registry_fetches_symbol_constraints_unchecked() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let constraints = registry
        .fetch_symbol_constraints_unchecked("alpaca-paper", "AAPL")
        .unwrap();
    assert!(constraints.fractional_entry_supported.is_none());
    assert!(constraints.quantity_step.is_none());
    assert!(constraints.min_quantity.is_none());
    assert!(constraints.min_notional_usd.is_none());
}

#[test]
fn connector_registry_submits_orders_for_ready_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "alpaca-paper",
        ConnectorKind::Alpaca,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let request = ExecutionRequest {
        instance_id: "aapl".to_owned(),
        symbol: "AAPL".to_owned(),
        timestamp,
        intent: TradeIntent::OpenLong,
        price: 123.45,
        quantity: 1.0,
    };

    let accepted = registry.submit_order("alpaca-paper", &request).unwrap();
    assert!(accepted.client_order_id.starts_with("alpaca-"));
}

#[test]
fn connector_registry_blocks_order_submission_for_degraded_account() {
    let registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();

    let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let request = ExecutionRequest {
        instance_id: "btc".to_owned(),
        symbol: "BTCUSDT".to_owned(),
        timestamp,
        intent: TradeIntent::OpenLong,
        price: 42_000.0,
        quantity: 0.01,
    };

    assert!(matches!(
        registry.submit_order("binance-paper", &request),
        Err(ConnectorError::Unavailable { .. })
    ));
}

#[test]
fn connector_registry_requires_demo_mode_for_binance_paper_accounts() {
    let no_demo_registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-paper",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        false,
    )])
    .unwrap();
    let no_demo_status = no_demo_registry
        .status_for_account("binance-paper")
        .unwrap();
    assert!(matches!(no_demo_status.state, ConnectionState::Degraded));
    assert!(no_demo_status.message.contains("requires demo mode"));

    let demo_registry = ConnectorRegistry::from_accounts(vec![test_account(
        "binance-demo",
        ConnectorKind::Binance,
        ExecutionMode::Paper,
        true,
    )])
    .unwrap();
    let demo_status = demo_registry.status_for_account("binance-demo").unwrap();
    assert!(matches!(demo_status.state, ConnectionState::Connected));
}

#[test]
fn resilience_policy_applies_bounded_backoff() {
    let policy = descriptor_for(ConnectorKind::Binance).resilience;
    assert_eq!(policy.reconnect_delay_ms(1), 300);
    assert!(policy.reconnect_delay_ms(2) >= policy.reconnect_delay_ms(1));
    assert_eq!(
        policy.reconnect_delay_ms(32),
        policy.reconnect_max_backoff_ms
    );
}

#[test]
fn resilience_state_tracks_reconnect_and_throttle_windows() {
    let policy = descriptor_for(ConnectorKind::Alpaca).resilience;
    let mut state = ConnectorResilienceState::default();
    let now_ms = 1_000;

    state.note_disconnect(now_ms, policy);
    assert_eq!(state.consecutive_failures, 1);
    assert_eq!(state.last_disconnect_at_ms, Some(now_ms));
    assert!(!state.can_attempt_reconnect(now_ms));

    let reconnect_at_ms = state
        .next_reconnect_at_ms
        .expect("disconnect should set reconnect boundary");
    assert!(state.can_attempt_reconnect(reconnect_at_ms));

    state.note_rate_limit(reconnect_at_ms, 2_000);
    assert!(!state.can_attempt_reconnect(reconnect_at_ms + 1_000));
    assert!(state.can_attempt_reconnect(reconnect_at_ms + 2_000));

    state.note_reconnect_success();
    assert_eq!(state.consecutive_failures, 0);
    assert!(state.can_attempt_reconnect(reconnect_at_ms));
}
