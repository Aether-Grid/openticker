use super::{
    ConfirmedBarReplayMode, ConfirmedBarReplayResult, InstanceWarmupState, LaneManualOpsEngine,
    LanePollingContext, LanePollingEngine, LaneRecoveryState, LaneRuntimeState, ManualCloseContext,
    ManualCloseOutcome, ManualCloseSignalOutcome, ManualCloseSignalRisk, RecoveryPageApplied,
    RecoveryStartKind, accepted_order_fee_entry, advance_lane_polling_once, advance_warmup_state,
    apply_process_bar_fill_state, close_lane_position, complete_lane_recovery_state,
    effective_position_quantity, mark_lane_out_of_sync_state, record_recovery_no_progress_state,
    record_warmup_failure, start_lane_recovery_state, sync_remote_position_quantity,
    sync_runtime_fields_from_inventory, validate_recovery_bars,
};
use openticker_config::{
    BudgetConfig, ExecutionConstraintsConfig, InstanceConfig, InstanceRiskConfig, RiskOverrides,
    SignalMode,
};
use openticker_connectors::ConfirmedBarPage;
use openticker_core::{ExecutionMode, MarketType, OhlcvBar, Timeframe, TradeIntent};
use openticker_execution::{AcceptedOrder, OrderSide, OrderType};
use openticker_instance::build_runtime_strategy;
use openticker_ledger::InventoryState;
use openticker_risk::{RiskDecision, RiskLimits};

const FLOAT_ASSERT_EPSILON: f64 = 1e-9;

fn assert_f64_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < FLOAT_ASSERT_EPSILON,
        "expected {actual} to be within {FLOAT_ASSERT_EPSILON} of {expected}"
    );
}

fn assert_opt_f64_close(actual: Option<f64>, expected: f64) {
    assert!(
        actual.is_some_and(|value| (value - expected).abs() < FLOAT_ASSERT_EPSILON),
        "expected {actual:?} to contain a value within {FLOAT_ASSERT_EPSILON} of {expected}"
    );
}

#[test]
fn lane_runtime_state_round_trips_storage_values() {
    assert_eq!(
        LaneRuntimeState::from_storage_value(LaneRuntimeState::Running.as_storage_value()),
        Some(LaneRuntimeState::Running)
    );
    assert_eq!(LaneRuntimeState::from_storage_value("unknown"), None);
}

#[test]
fn warmup_state_is_ready_only_when_no_bars_are_required() {
    assert!(InstanceWarmupState::new(0).ready);
    assert!(!InstanceWarmupState::new(10).ready);
}

#[test]
fn accepted_order_fee_entry_filters_invalid_fee_values() {
    let valid = AcceptedOrder {
        client_order_id: "order-1".to_owned(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        price: 100.0,
        quantity: 1.0,
        fee_asset: Some("USD".to_owned()),
        fee_amount: Some(0.5),
        fee_normalized_usd: Some(0.5),
    };
    let invalid = AcceptedOrder {
        fee_amount: Some(0.0),
        ..valid.clone()
    };

    let fee = accepted_order_fee_entry(&valid).expect("fee entry should exist");
    assert_eq!(fee.asset, "USD");
    assert_f64_close(fee.amount, 0.5);
    assert_opt_f64_close(fee.normalized_usd, 0.5);
    assert!(accepted_order_fee_entry(&invalid).is_none());
}

#[test]
fn warmup_helpers_record_failure_and_ready_transition() {
    let mut warmup = InstanceWarmupState::new(2);
    record_warmup_failure(&mut warmup, "fetch failed".to_owned());
    assert_eq!(warmup.last_error.as_deref(), Some("fetch failed"));

    let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let first = advance_warmup_state(&mut warmup, timestamp).expect("advance should apply");
    assert_eq!(first.loaded_bars, 1);
    assert!(!first.became_ready);
    let second = advance_warmup_state(&mut warmup, timestamp).expect("advance should apply");
    assert_eq!(second.loaded_bars, 2);
    assert!(second.became_ready);
    assert!(warmup.ready);
    assert!(advance_warmup_state(&mut warmup, timestamp).is_none());
}

#[test]
fn recovery_helpers_track_start_completion_and_failure() {
    let mut lane = test_lane_runtime();
    lane.last_dispatched_bar_timestamp = Some(
        chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc),
    );
    let target = chrono::DateTime::parse_from_rfc3339("2030-01-01T01:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);

    let kind = start_lane_recovery_state(&mut lane, target, 123);
    assert_eq!(kind, RecoveryStartKind::Started);
    assert_eq!(lane.recovery_state, LaneRecoveryState::CatchingUp);
    assert_eq!(lane.recovery_started_at_ms, Some(123));

    let no_progress = record_recovery_no_progress_state(&mut lane, false, 3);
    assert_eq!(no_progress.cycles, 1);
    assert!(!no_progress.should_fail);

    mark_lane_out_of_sync_state(&mut lane, "stalled");
    assert_eq!(lane.recovery_state, LaneRecoveryState::OutOfSync);
    assert_eq!(lane.recovery_last_error.as_deref(), Some("stalled"));

    let resumed = start_lane_recovery_state(&mut lane, target, 456);
    assert_eq!(resumed, RecoveryStartKind::Resumed);
    lane.recovery_last_progress_timestamp = Some(target);
    let completed = complete_lane_recovery_state(&mut lane);
    assert_eq!(completed, Some(target));
    assert_eq!(lane.recovery_state, LaneRecoveryState::Healthy);
    assert!(lane.recovery_target_timestamp.is_none());
}

#[test]
fn process_bar_fill_state_updates_open_position_and_position_record() {
    let mut lane = test_lane_runtime();
    let bar = test_bar(101.0);
    let accepted_order = AcceptedOrder {
        client_order_id: "order-1".to_owned(),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        price: 100.0,
        quantity: 2.0,
        fee_asset: Some("USD".to_owned()),
        fee_amount: Some(0.5),
        fee_normalized_usd: Some(0.5),
    };

    let mutation = apply_process_bar_fill_state(
        &mut lane,
        &bar,
        true,
        Some(&accepted_order),
        None,
        &RiskDecision::Allow(TradeIntent::OpenLong),
    )
    .expect("fill application should succeed");

    assert!(mutation.released_notional_usd.is_none());
    let position_record = mutation
        .position_record
        .expect("position record should exist");
    assert!(position_record.has_position);
    assert!((position_record.quantity - 2.0).abs() < 1e-9);
    assert!(lane.has_position);
    assert!((lane.position_quantity - 2.0).abs() < 1e-9);
    assert!(lane.entry_price.is_some());
}

#[test]
fn process_bar_fill_state_releases_closed_notional_and_tracks_loss() {
    let mut lane = test_lane_runtime();
    lane.has_position = true;
    lane.position_quantity = 2.0;
    lane.entry_price = Some(100.0);
    lane.position_notional_usd = 200.0;
    lane.inventory = InventoryState::from_position_state(2.0, Some(100.0), 0.0);
    let bar = test_bar(90.0);
    let accepted_order = AcceptedOrder {
        client_order_id: "order-2".to_owned(),
        side: OrderSide::Sell,
        order_type: OrderType::Market,
        price: 90.0,
        quantity: 1.0,
        fee_asset: None,
        fee_amount: None,
        fee_normalized_usd: None,
    };

    let mutation = apply_process_bar_fill_state(
        &mut lane,
        &bar,
        false,
        Some(&accepted_order),
        None,
        &RiskDecision::Allow(TradeIntent::ReduceLong),
    )
    .expect("fill application should succeed");

    assert_opt_f64_close(mutation.released_notional_usd, 90.0);
    assert!(lane.daily_loss_pct_accumulated > 0.0);
    let position_record = mutation
        .position_record
        .expect("position record should exist");
    assert!(position_record.has_position);
    assert!((position_record.quantity - 1.0).abs() < 1e-9);
}

#[test]
fn rejected_process_bar_sets_reject_cooldown_without_inventory_mutation() {
    let mut lane = test_lane_runtime();
    let bar = test_bar(101.0);

    let mutation = apply_process_bar_fill_state(
        &mut lane,
        &bar,
        false,
        None,
        None,
        &RiskDecision::Reject {
            reason: "risk_limit",
        },
    )
    .expect("reject application should succeed");

    assert!(mutation.position_record.is_none());
    assert!(mutation.released_notional_usd.is_none());
    assert!(lane.cooldown_until_ms.is_some());
}

#[test]
fn remote_position_sync_updates_runtime_fields() {
    let mut lane = test_lane_runtime();
    lane.entry_price = Some(100.0);

    assert!(sync_remote_position_quantity(&mut lane, 3.0));
    assert!(lane.has_position);
    assert!((lane.position_quantity - 3.0).abs() < 1e-9);
    assert!((lane.position_notional_usd - 300.0).abs() < 1e-9);

    assert!(sync_remote_position_quantity(&mut lane, 0.0));
    assert!(!lane.has_position);
    assert_eq!(lane.entry_price, None);
    assert_f64_close(lane.position_notional_usd, 0.0);
}

#[test]
fn recovery_bar_validation_rejects_out_of_order_and_future_bars() {
    let start_after = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);
    let end_at = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:02:00Z")
        .expect("timestamp should parse")
        .with_timezone(&chrono::Utc);

    let out_of_order = vec![test_bar_at(101.0, 1), test_bar_at(102.0, 0)];
    assert!(validate_recovery_bars(Some(start_after), end_at, &out_of_order).is_err());

    let future_bar = vec![test_bar_at(101.0, 3)];
    assert!(validate_recovery_bars(Some(start_after), end_at, &future_bar).is_err());
}

#[test]
fn manual_close_skips_when_lane_is_already_flat() {
    let mut engine = MockManualCloseEngine {
        context: ManualCloseContext {
            bot_id: "bot-a".to_owned(),
            account_id: "acct".to_owned(),
            reconciliation_remote_snapshot: false,
            has_local_position: false,
        },
        ..MockManualCloseEngine::default()
    };

    let outcome = close_lane_position(&mut engine, "bot-a").expect("manual close should succeed");

    assert!(matches!(outcome, ManualCloseOutcome::AlreadyFlat));
    assert!(!engine.fetch_latest_bar_called);
    assert!(!engine.process_manual_close_signal_called);
}

#[test]
fn manual_close_uses_remote_sync_before_submitting_signal() {
    let mut engine = MockManualCloseEngine {
        context: ManualCloseContext {
            bot_id: "bot-a".to_owned(),
            account_id: "acct".to_owned(),
            reconciliation_remote_snapshot: true,
            has_local_position: false,
        },
        remote_has_position: true,
        latest_bar: test_bar(101.0),
        signal_outcome: ManualCloseSignalOutcome {
            intent: TradeIntent::CloseLong,
            risk: ManualCloseSignalRisk::Allowed,
        },
        ..MockManualCloseEngine::default()
    };

    let outcome = close_lane_position(&mut engine, "bot-a").expect("manual close should succeed");

    assert!(engine.sync_remote_position_called);
    assert!(matches!(
        outcome,
        ManualCloseOutcome::Processed {
            intent: TradeIntent::CloseLong,
            risk: ManualCloseSignalRisk::Allowed,
            price,
            ..
        } if (price - 101.0).abs() < 1e-9
    ));
}

#[test]
fn effective_position_quantity_never_fabricates_when_quantity_is_zero() {
    let mut lane = test_lane_runtime();
    // Inconsistent state: the lane claims a position while both quantity
    // sources are zero. This previously returned a fabricated `1.0`, which
    // corrupted notional and order-sizing math downstream.
    lane.has_position = true;
    lane.position_quantity = 0.0;
    lane.inventory = InventoryState::default();

    assert_f64_close(effective_position_quantity(&lane), 0.0);

    // Sanity: a genuine position still reports its real quantity.
    lane.position_quantity = 2.5;
    assert_f64_close(effective_position_quantity(&lane), 2.5);
}

#[test]
fn sync_runtime_fields_records_inconsistency_via_recovery_last_error() {
    let mut lane = test_lane_runtime();
    // Construct the genuinely inconsistent *pre-sync* state that can arise
    // across the public boundary (e.g. a reconciliation assessment that
    // resolves `has_position = true` with a ~0 resolved quantity): the lane
    // claims a position while BOTH effective quantity sources are ~0.
    lane.has_position = true;
    lane.position_quantity = 0.0;
    lane.inventory = InventoryState::default();
    lane.recovery_last_error = None;

    sync_runtime_fields_from_inventory(&mut lane, Some(100.0));

    // The sync collapses the lane to a coherent flat state ...
    assert!(!lane.has_position);
    assert_f64_close(lane.position_quantity, 0.0);
    assert_f64_close(lane.position_notional_usd, 0.0);
    // ... and the read-only accessor still refuses to fabricate a quantity.
    assert_f64_close(effective_position_quantity(&lane), 0.0);
    // ... while the prior divergence is recorded on a release-visible
    // channel so an operator can see it in production.
    let recorded = lane
        .recovery_last_error
        .as_deref()
        .expect("inconsistency should be recorded via recovery_last_error");
    assert!(
        recorded.contains("position-quantity invariant violated"),
        "unexpected recovery_last_error: {recorded}"
    );
    assert!(
        recorded.contains("symbol=AAPL") && recorded.contains("instance=bot-a"),
        "recovery_last_error should carry debug context: {recorded}"
    );
}

#[test]
fn sync_runtime_fields_leaves_recovery_last_error_clear_when_consistent() {
    let mut lane = test_lane_runtime();
    // A genuine flat lane (no claimed position, zero quantity) is NOT an
    // inconsistency and must not be flagged.
    lane.recovery_last_error = None;
    sync_runtime_fields_from_inventory(&mut lane, Some(100.0));
    assert!(
        lane.recovery_last_error.is_none(),
        "a consistent flat lane must not flag an invariant violation"
    );

    // A lane that closes out normally (cached quantity still non-zero at
    // entry while inventory has zeroed) is the expected close transition,
    // not the both-sources-zero anomaly, so it must not be flagged either.
    lane.has_position = true;
    lane.position_quantity = 3.0;
    lane.inventory = InventoryState::default();
    lane.recovery_last_error = None;
    sync_runtime_fields_from_inventory(&mut lane, Some(100.0));
    assert!(
        lane.recovery_last_error.is_none(),
        "a normal close (cached quantity non-zero at entry) is not an anomaly"
    );
}

#[derive(Default)]
struct StubPollingEngine {
    context: Option<LanePollingContext>,
}

impl LanePollingEngine for StubPollingEngine {
    type Error = String;
    type Outcome = ();

    fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn polling_context(&self, _instance_id: &str) -> Result<LanePollingContext, Self::Error> {
        self.context
            .clone()
            .ok_or_else(|| "no polling context configured".to_owned())
    }

    fn invariant_violation(&self, instance_id: &str, reason: &str) -> Self::Error {
        format!("invariant violation for `{instance_id}`: {reason}")
    }

    fn replay_confirmed_bar(
        &mut self,
        _instance_id: &str,
        _bar: &OhlcvBar,
        _mode: ConfirmedBarReplayMode,
    ) -> Result<ConfirmedBarReplayResult<Self::Outcome>, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn fetch_latest_bar(
        &mut self,
        _instance_id: &str,
        _account_id: &str,
        _data_connector: &str,
        _symbol: &str,
        _timeframe: Timeframe,
    ) -> Result<OhlcvBar, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn fetch_latest_confirmed_bar_timestamp(
        &mut self,
        _instance_id: &str,
        _account_id: &str,
        _data_connector: &str,
        _symbol: &str,
        _timeframe: Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn fetch_confirmed_bars_range(
        &mut self,
        _instance_id: &str,
        _account_id: &str,
        _data_connector: &str,
        _symbol: &str,
        _timeframe: Timeframe,
        _start_after: Option<chrono::DateTime<chrono::Utc>>,
        _end_at: chrono::DateTime<chrono::Utc>,
        _limit: usize,
    ) -> Result<ConfirmedBarPage, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn start_lane_recovery(
        &mut self,
        _instance_id: &str,
        _target: chrono::DateTime<chrono::Utc>,
        _now_ms: i64,
    ) -> Result<(), Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn complete_lane_recovery(
        &mut self,
        _instance_id: &str,
        _reason: &str,
    ) -> Result<(), Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn mark_lane_out_of_sync(
        &mut self,
        _instance_id: &str,
        _reason: &str,
    ) -> Result<(), Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn last_dispatched_bar_timestamp(
        &self,
        _instance_id: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn apply_recovery_page(
        &mut self,
        _instance_id: &str,
        _bars: &[OhlcvBar],
    ) -> Result<usize, Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn record_recovery_page_applied(
        &mut self,
        _instance_id: &str,
        _detail: RecoveryPageApplied,
    ) -> Result<(), Self::Error> {
        unimplemented!("not exercised by this test")
    }

    fn record_recovery_no_progress(
        &mut self,
        _instance_id: &str,
        _target: chrono::DateTime<chrono::Utc>,
        _exhausted: bool,
    ) -> Result<(), Self::Error> {
        unimplemented!("not exercised by this test")
    }
}

#[test]
fn advance_lane_polling_once_returns_err_when_catching_up_without_target() {
    let mut engine = StubPollingEngine {
        context: Some(LanePollingContext {
            account_id: "acct".to_owned(),
            data_connector: "paper".to_owned(),
            symbol: "AAPL".to_owned(),
            timeframe: Timeframe::M1,
            // Invariant violation: CatchingUp with no recovery target.
            recovery_state: LaneRecoveryState::CatchingUp,
            last_dispatched: None,
            recovery_target: None,
        }),
    };

    let result = advance_lane_polling_once(&mut engine, "bot-a", 2, 4, 0);
    let error = result.expect_err("missing recovery target must surface as an error, not a panic");
    assert!(
        error.contains("CatchingUp without a recovery target"),
        "unexpected error message: {error}"
    );
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
struct MockManualCloseEngine {
    context: ManualCloseContext,
    remote_has_position: bool,
    latest_bar: OhlcvBar,
    signal_outcome: ManualCloseSignalOutcome,
    sync_remote_position_called: bool,
    fetch_latest_bar_called: bool,
    process_manual_close_signal_called: bool,
}

impl Default for MockManualCloseEngine {
    fn default() -> Self {
        Self {
            context: ManualCloseContext {
                bot_id: "bot-a".to_owned(),
                account_id: "acct".to_owned(),
                reconciliation_remote_snapshot: false,
                has_local_position: false,
            },
            remote_has_position: false,
            latest_bar: test_bar(100.0),
            signal_outcome: ManualCloseSignalOutcome {
                intent: TradeIntent::NoOp,
                risk: ManualCloseSignalRisk::Allowed,
            },
            sync_remote_position_called: false,
            fetch_latest_bar_called: false,
            process_manual_close_signal_called: false,
        }
    }
}

impl LaneManualOpsEngine for MockManualCloseEngine {
    type Error = &'static str;

    fn manual_close_context(&self, _instance_id: &str) -> Result<ManualCloseContext, Self::Error> {
        Ok(self.context.clone())
    }

    fn sync_remote_position_for_manual_close(
        &mut self,
        _instance_id: &str,
        _account_id: &str,
    ) -> Result<bool, Self::Error> {
        self.sync_remote_position_called = true;
        Ok(self.remote_has_position)
    }

    fn fetch_latest_bar_for_manual_close(
        &mut self,
        _instance_id: &str,
    ) -> Result<OhlcvBar, Self::Error> {
        self.fetch_latest_bar_called = true;
        Ok(self.latest_bar.clone())
    }

    fn process_manual_close_signal(
        &mut self,
        _instance_id: &str,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ManualCloseSignalOutcome, Self::Error> {
        self.process_manual_close_signal_called = true;
        assert!((price - self.latest_bar.close).abs() < 1e-9);
        assert_eq!(timestamp, self.latest_bar.timestamp);
        Ok(self.signal_outcome.clone())
    }
}

fn test_lane_runtime() -> super::LaneRuntime {
    let config = InstanceConfig {
        id: "bot-a".to_owned(),
        enabled: true,
        market: MarketType::Equities,
        account: "acct".to_owned(),
        execution_connector: "paper".to_owned(),
        data_connector: "paper".to_owned(),
        timeframe: Timeframe::M1,
        symbols: vec!["AAPL".to_owned()],
        budget: BudgetConfig { pct: 25.0 },
        indicators: vec![],
        strategy: "single_indicator_signal".to_owned(),
        signal_mode: SignalMode::ConfirmedOnly,
        execution_constraints: ExecutionConstraintsConfig::default(),
        polling_enabled: true,
        polling_interval_ms: 60_000,
        risk: InstanceRiskConfig {
            profile: "default".to_owned(),
            overrides: RiskOverrides::default(),
        },
        warmup_target_bars: Some(0),
        allow_live: false,
    };

    super::LaneRuntime {
        config: config.clone(),
        lane_symbol: "AAPL".to_owned(),
        execution_mode: ExecutionMode::Paper,
        state: LaneRuntimeState::Stopped,
        resume_after_startup_reconcile: false,
        indicators: Vec::new(),
        strategy: build_runtime_strategy(&config).expect("strategy should build"),
        bar_builder: openticker_data::BarBuilder::new("AAPL".to_owned(), Timeframe::M1),
        risk_limits: RiskLimits {
            max_daily_loss_pct: 5.0,
            max_open_positions: 5,
            max_order_notional_usd: 1_000.0,
            max_spread_bps: 20,
            max_slippage_bps: 20,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        },
        target_order_notional_usd: 100.0,
        inventory: InventoryState::default(),
        has_position: false,
        position_quantity: 0.0,
        position_notional_usd: 0.0,
        entry_price: None,
        realized_pnl_usd: 0.0,
        daily_loss_pct_accumulated: 0.0,
        last_loss_reset_date: None,
        cooldown_until_ms: None,
        reconciliation_blocked: false,
        remote_net_qty: None,
        aggregate_managed_qty: 0.0,
        external_delta_qty: None,
        managed_remote_open_orders: 0,
        external_remote_open_orders: 0,
        warmup: InstanceWarmupState::new(0),
        recovery_state: LaneRecoveryState::Healthy,
        recovery_started_at_ms: None,
        recovery_target_timestamp: None,
        recovery_last_progress_timestamp: None,
        recovery_last_error: None,
        recovery_consecutive_no_progress_cycles: 0,
        last_recovered_at_timestamp: None,
        last_dispatched_bar_timestamp: None,
        last_stream_update: None,
        connector_execution_constraints: None,
        connector_fractional_entry_supported: None,
        connector_execution_constraints_initialized: false,
    }
}

fn test_bar(close: f64) -> OhlcvBar {
    test_bar_at(close, 0)
}

fn test_bar_at(close: f64, minute_offset: i64) -> OhlcvBar {
    OhlcvBar {
        timestamp: chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc)
            + chrono::Duration::minutes(minute_offset),
        open: close,
        high: close,
        low: close,
        close,
        volume: 0.0,
    }
}
