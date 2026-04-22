use super::*;
use crate::test_support::{fixture_bundle, test_bar_at};
use openticker_core::{MarketType, TradeIntent};
use openticker_lane::InstanceWarmupState;

#[test]
fn startup_warmup_marks_instance_ready_and_emits_events() {
    let config = fixture_bundle();
    let runtime = Runtime::from_config(&config);

    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert!(summary.warmup.ready);
    assert_eq!(summary.warmup.loaded_bars, summary.warmup.required_bars);
    assert!(summary.warmup.last_error.is_none());

    let status = runtime.status();
    assert_eq!(status.warmup_ready_instances, 1);
    assert_eq!(status.warmup_pending_instances, 0);
    assert_eq!(status.warmup_failed_instances, 0);

    let events = runtime
        .recent_events_by_scope_and_entity("warmup", "aapl", 500)
        .expect("warmup events should load");
    assert!(events.iter().any(|event| event.kind == "warmup.started"));
    assert!(events.iter().any(|event| event.kind == "warmup.progress"));
    assert!(events.iter().any(|event| event.kind == "warmup.ready"));
}

#[test]
fn startup_warmup_failure_surfaces_pending_error_state() {
    let mut config = fixture_bundle();
    config.accounts[0].kind = "binance".to_owned();
    config.accounts[0].use_demo_mode = false;
    config.instances[0].market = MarketType::Crypto;
    config.instances[0].symbols = vec!["BTCUSDT".to_owned()];
    config.instances[0].data_connector = "binance".to_owned();
    config.instances[0].execution_connector = "binance".to_owned();

    let runtime = Runtime::from_config(&config);
    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert!(!summary.warmup.ready);
    assert!(summary.warmup.last_error.is_some());

    let status = runtime.status();
    assert_eq!(status.warmup_ready_instances, 0);
    assert_eq!(status.warmup_pending_instances, 1);
    assert_eq!(status.warmup_failed_instances, 1);
}

#[test]
fn start_instance_backfills_pending_warmup_before_running() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    let warmup_ready_before = runtime
        .recent_events_by_scope_and_entity("warmup", "aapl", 200)
        .expect("warmup events should load")
        .into_iter()
        .filter(|event| event.kind == "warmup.ready")
        .count();

    {
        let instance = runtime.instance_mut("aapl").expect("instance should exist");
        instance.warmup = InstanceWarmupState {
            required_bars: 2,
            loaded_bars: 0,
            ready: false,
            last_error: Some("startup warmup backfill unavailable".to_owned()),
            last_warmup_timestamp: None,
        };
        instance.last_dispatched_bar_timestamp = None;
    }

    let started = runtime
        .start_instance("aapl")
        .expect("instance should start with warmup backfill");
    assert_eq!(started.state, crate::LaneRuntimeState::Running);
    assert!(started.warmup.ready);
    assert_eq!(started.warmup.loaded_bars, 2);
    assert!(started.warmup.last_error.is_none());

    let events = runtime
        .recent_events_by_scope_and_entity("warmup", "aapl", 50)
        .expect("warmup events should load");
    let warmup_ready_after = events
        .iter()
        .filter(|event| event.kind == "warmup.ready")
        .count();
    assert_eq!(warmup_ready_after, warmup_ready_before + 1);
    assert!(
        events.iter().any(
            |event| event.kind == "warmup.ready" && event.payload.contains(r#"source":"start"#)
        )
    );
}

#[test]
fn resume_instance_backfills_pending_warmup_before_running() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("instance should start");
    runtime
        .pause_instance("aapl")
        .expect("instance should pause");
    let warmup_ready_before = runtime
        .recent_events_by_scope_and_entity("warmup", "aapl", 200)
        .expect("warmup events should load")
        .into_iter()
        .filter(|event| event.kind == "warmup.ready")
        .count();

    {
        let instance = runtime.instance_mut("aapl").expect("instance should exist");
        instance.warmup = InstanceWarmupState {
            required_bars: 1,
            loaded_bars: 0,
            ready: false,
            last_error: Some("warmup reset for resume".to_owned()),
            last_warmup_timestamp: None,
        };
        instance.last_dispatched_bar_timestamp = None;
    }

    let resumed = runtime
        .resume_instance("aapl")
        .expect("instance should resume with warmup backfill");
    assert_eq!(resumed.state, crate::LaneRuntimeState::Running);
    assert!(resumed.warmup.ready);
    assert_eq!(resumed.warmup.loaded_bars, 1);
    assert!(resumed.warmup.last_error.is_none());

    let events = runtime
        .recent_events_by_scope_and_entity("warmup", "aapl", 50)
        .expect("warmup events should load");
    let warmup_ready_after = events
        .iter()
        .filter(|event| event.kind == "warmup.ready")
        .count();
    assert_eq!(warmup_ready_after, warmup_ready_before + 1);
    assert!(
        events
            .iter()
            .any(|event| event.kind == "warmup.ready"
                && event.payload.contains(r#"source":"resume"#))
    );
}

#[test]
fn warmup_replay_does_not_double_apply_confirmed_bar_timestamp() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("instance should start");

    {
        let instance = runtime.instance_mut("aapl").expect("instance should exist");
        instance.warmup = InstanceWarmupState {
            required_bars: 2,
            loaded_bars: 0,
            ready: false,
            last_error: Some("startup warmup backfill unavailable".to_owned()),
            last_warmup_timestamp: None,
        };
        instance.last_dispatched_bar_timestamp = None;
    }

    let bar = test_bar_at("2030-01-01T00:00:00Z", 100.0);
    let first = runtime
        .process_bar("aapl", &bar, SignalPhase::Confirmed)
        .expect("first bar should process");
    assert_eq!(first.intent, TradeIntent::NoOp);

    let second = runtime
        .process_bar("aapl", &bar, SignalPhase::Confirmed)
        .expect("duplicate bar should process as no-op");
    assert_eq!(second.intent, TradeIntent::NoOp);

    let summary = runtime.get_instance("aapl").expect("instance should exist");
    assert_eq!(summary.warmup.loaded_bars, 1);
    assert!(!summary.warmup.ready);
    assert_eq!(
        summary.warmup.last_warmup_timestamp.as_deref(),
        Some("2030-01-01T00:00:00+00:00")
    );
}
