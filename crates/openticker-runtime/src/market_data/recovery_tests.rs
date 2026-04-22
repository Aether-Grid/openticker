use super::super::recovery_engine::MAX_RECOVERY_NO_PROGRESS_CYCLES;
use super::*;
use crate::test_support::{fixture_bundle, fixture_bundle_with_timeframe, test_bar};
use openticker_core::Timeframe;

const RECOVERY_MATRIX_TIMEFRAMES: [Timeframe; 7] = [
    Timeframe::M1,
    Timeframe::M5,
    Timeframe::M15,
    Timeframe::M30,
    Timeframe::H1,
    Timeframe::H4,
    Timeframe::D1,
];

fn timeframe_duration(timeframe: Timeframe) -> chrono::Duration {
    chrono::Duration::from_std(timeframe.duration())
        .expect("supported timeframe durations should fit chrono")
}

fn latest_confirmed_timestamp_for(
    runtime: &Runtime,
    timeframe: Timeframe,
) -> chrono::DateTime<chrono::Utc> {
    runtime
        .connector_gateway()
        .fetch_latest_confirmed_bar_timestamp("aapl", "alpaca-paper", "alpaca", "AAPL", timeframe)
        .expect("latest confirmed timestamp should load")
        .expect("fixture connector should provide a latest confirmed timestamp")
}

fn recovery_ready_runtime(timeframe: Timeframe) -> Runtime {
    let config = fixture_bundle_with_timeframe(timeframe);
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("instance should start");
    runtime
}

fn latest_confirmed_timestamp(runtime: &Runtime) -> chrono::DateTime<chrono::Utc> {
    latest_confirmed_timestamp_for(runtime, Timeframe::M1)
}

#[test]
fn backlog_recovery_replays_state_only_and_auto_resumes() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("instance should start");

    let latest_target = latest_confirmed_timestamp(&runtime);
    {
        let lane = runtime.instance_mut("aapl").expect("lane should exist");
        lane.last_dispatched_bar_timestamp = Some(latest_target - chrono::Duration::minutes(3));
    }

    let outcomes = runtime
        .poll_instance_once("aapl")
        .expect("recovery poll should succeed");
    assert!(outcomes.is_empty());
    assert!(
        runtime
            .recent_orders(10)
            .expect("orders should load")
            .is_empty()
    );

    let summary = runtime
        .get_instance("aapl")
        .expect("instance summary should load");
    assert_eq!(summary.recovery.state, LaneRecoveryState::Healthy);
    assert!(summary.recovery.last_error.is_none());
    assert!(summary.recovery.last_recovered_at_timestamp.is_some());

    let events = runtime
        .recent_events_by_scope_and_entity("poll", "aapl", 50)
        .expect("poll events should load");
    assert!(
        events
            .iter()
            .any(|event| event.kind == "poll.recovery.started")
    );
    assert!(
        events
            .iter()
            .any(|event| event.kind == "poll.recovery.page_applied")
    );
    assert!(
        events
            .iter()
            .any(|event| event.kind == "poll.recovery.completed")
    );
}

#[test]
fn exhausted_recovery_moves_lane_out_of_sync_and_blocks_trading() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("instance should start");

    let latest_target = latest_confirmed_timestamp(&runtime);
    {
        let lane = runtime.instance_mut("aapl").expect("lane should exist");
        lane.recovery_state = LaneRecoveryState::CatchingUp;
        lane.recovery_target_timestamp = Some(latest_target + chrono::Duration::minutes(3));
        lane.last_dispatched_bar_timestamp = Some(latest_target - chrono::Duration::minutes(1));
    }

    let outcomes = runtime
        .poll_instance_once("aapl")
        .expect("recovery poll should not hard-fail on exhaustion");
    assert!(outcomes.is_empty());

    let summary = runtime
        .get_instance("aapl")
        .expect("instance summary should load");
    assert_eq!(summary.recovery.state, LaneRecoveryState::OutOfSync);
    assert!(summary.recovery.trading_blocked_by_recovery);
    assert!(summary.recovery.last_error.is_some());

    let blocked = runtime
        .process_bar("aapl", &test_bar(101.0), SignalPhase::Confirmed)
        .expect("recovery-blocked bar should return a no-op outcome");
    assert_eq!(blocked.intent, TradeIntent::NoOp);
    assert_eq!(
        blocked.strategy_rationale.as_deref(),
        Some("recovery_pending")
    );
}

#[test]
fn recovery_matrix_auto_resumes_across_supported_timeframes() {
    for timeframe in RECOVERY_MATRIX_TIMEFRAMES {
        for gap_bars in [2_i32, 250_i32] {
            let mut runtime = recovery_ready_runtime(timeframe);
            let latest_target = latest_confirmed_timestamp_for(&runtime, timeframe);
            let gap_duration = timeframe_duration(timeframe) * gap_bars;

            {
                let lane = runtime.instance_mut("aapl").expect("lane should exist");
                lane.last_dispatched_bar_timestamp = Some(latest_target - gap_duration);
            }

            let advance = runtime
                .poll_instance_once_detailed("aapl")
                .expect("recovery poll should succeed");

            assert!(
                advance.outcomes.is_empty(),
                "timeframe {timeframe} gap {gap_bars} should remain state-only during recovery"
            );
            assert_eq!(
                advance.recorded_bars.len(),
                usize::try_from(gap_bars).expect("gap bars should fit usize"),
                "timeframe {timeframe} should recover every missed confirmed bar"
            );
            assert!(
                advance
                    .recorded_bars
                    .windows(2)
                    .all(|window| window[0].timestamp < window[1].timestamp)
            );
            assert_eq!(
                runtime
                    .instance("aapl")
                    .expect("lane should exist")
                    .last_dispatched_bar_timestamp,
                Some(latest_target),
                "timeframe {timeframe} should advance to the frozen recovery target"
            );

            let summary = runtime
                .get_instance("aapl")
                .expect("instance summary should load");
            assert_eq!(summary.recovery.state, LaneRecoveryState::Healthy);
            assert!(!summary.recovery.trading_blocked_by_recovery);
            assert!(summary.recovery.last_error.is_none());
        }
    }
}

#[test]
fn manual_tick_and_scheduler_recovery_match_across_supported_timeframes() {
    for timeframe in RECOVERY_MATRIX_TIMEFRAMES {
        let mut manual_runtime = recovery_ready_runtime(timeframe);
        let manual_target = latest_confirmed_timestamp_for(&manual_runtime, timeframe);
        {
            let lane = manual_runtime
                .instance_mut("aapl")
                .expect("lane should exist");
            lane.last_dispatched_bar_timestamp =
                Some(manual_target - timeframe_duration(timeframe) * 4);
        }
        let manual = manual_runtime
            .poll_instance_once_detailed("aapl")
            .expect("manual recovery should succeed");

        let mut scheduler_runtime = recovery_ready_runtime(timeframe);
        let scheduler_target = latest_confirmed_timestamp_for(&scheduler_runtime, timeframe);
        {
            let lane = scheduler_runtime
                .instance_mut("aapl")
                .expect("lane should exist");
            lane.last_dispatched_bar_timestamp =
                Some(scheduler_target - timeframe_duration(timeframe) * 4);
        }
        let stream_key = scheduler_runtime
            .effective_streams_for_dataplane()
            .into_iter()
            .find(|stream| stream.key.symbol == "AAPL")
            .expect("stream should exist")
            .key;
        let scheduled = scheduler_runtime
            .advance_stream_polling_once(&stream_key)
            .expect("scheduler recovery should succeed");

        let manual_timestamps = manual
            .recorded_bars
            .iter()
            .map(|bar| bar.timestamp)
            .collect::<Vec<_>>();
        let scheduled_timestamps = scheduled
            .recorded_bars
            .iter()
            .map(|bar| bar.timestamp)
            .collect::<Vec<_>>();
        assert_eq!(manual_timestamps, scheduled_timestamps);
        assert_eq!(manual.outcomes.len(), scheduled.outcomes.len());
        assert_eq!(
            manual_runtime.get_instance("aapl").unwrap().recovery.state,
            LaneRecoveryState::Healthy
        );
        assert_eq!(
            scheduler_runtime
                .get_instance("aapl")
                .unwrap()
                .recovery
                .state,
            LaneRecoveryState::Healthy
        );
    }
}

#[test]
fn repeated_no_progress_recovery_fails_closed() {
    let mut runtime = recovery_ready_runtime(Timeframe::M1);
    let target =
        latest_confirmed_timestamp_for(&runtime, Timeframe::M1) + chrono::Duration::minutes(5);
    {
        let lane = runtime.instance_mut("aapl").expect("lane should exist");
        lane.recovery_state = LaneRecoveryState::CatchingUp;
        lane.recovery_target_timestamp = Some(target);
        lane.recovery_started_at_ms = Some(crate::unix_now_ms());
    }

    for attempt in 1..MAX_RECOVERY_NO_PROGRESS_CYCLES {
        runtime
            .record_recovery_no_progress("aapl", target, false)
            .expect("no-progress bookkeeping should succeed");
        let summary = runtime
            .get_instance("aapl")
            .expect("instance summary should load");
        assert_eq!(summary.recovery.state, LaneRecoveryState::CatchingUp);
        assert_eq!(summary.recovery.consecutive_no_progress_cycles, attempt);
    }

    runtime
        .record_recovery_no_progress("aapl", target, false)
        .expect("terminal no-progress bookkeeping should succeed");
    let summary = runtime
        .get_instance("aapl")
        .expect("instance summary should load");
    assert_eq!(summary.recovery.state, LaneRecoveryState::OutOfSync);
    assert!(summary.recovery.trading_blocked_by_recovery);
    assert!(
        summary
            .recovery
            .last_error
            .as_deref()
            .is_some_and(|message| message.contains("no progress"))
    );
}
