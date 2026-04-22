use super::*;
use crate::ProcessBarRisk;
use crate::test_support::{fixture_bundle, replay_closes, test_bar_at};
use chrono::{Duration, Utc};
use openticker_core::TradeIntent;
use openticker_lane::InstanceWarmupState;

#[test]
fn process_bar_generates_signal_order_position_and_risk_events() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

    for close in replay_closes() {
        let bar = test_bar_at("2030-01-01T00:00:00Z", close);
        let _ = runtime
            .process_bar("aapl", &bar, SignalPhase::Confirmed)
            .expect("process_bar should succeed for fixture replay");
    }

    let signals = runtime.recent_signals(500).expect("signals should load");
    assert!(!signals.is_empty());
    assert!(signals.iter().all(|signal| !signal.signal.is_empty()));
    assert!(signals.iter().all(|signal| signal.metadata_json.is_some()));

    let intents = runtime.recent_intents(500).expect("intents should load");
    assert!(!intents.is_empty());
    assert!(
        intents
            .iter()
            .all(|intent| intent.strategy_rationale.is_some())
    );

    let risks = runtime
        .recent_risk_decisions(500)
        .expect("risk decisions should load");
    assert!(risks.iter().any(|decision| decision.decision == "allowed"));

    let risk_events = runtime
        .recent_events_by_scope("risk", 500)
        .expect("risk events should load");
    assert!(
        risk_events
            .iter()
            .any(|event| matches!(event.kind.as_str(), "risk.allowed" | "risk.rejected"))
    );
    for event in risk_events
        .iter()
        .filter(|event| matches!(event.kind.as_str(), "risk.allowed" | "risk.rejected"))
    {
        let detail = serde_json::from_str::<serde_json::Value>(&event.payload)
            .expect("risk event detail should be valid JSON");
        assert!(detail.get("symbol").is_some());
        assert!(detail.get("bar_timestamp").is_some());
        assert!(detail.get("intent").is_some());
        assert!(detail.get("decision").is_some());
        assert!(detail.get("reason").is_some());
    }

    let orders = runtime.recent_orders(500).expect("orders should load");
    assert!(orders.iter().any(|order| order.status == "submitted"));
    assert!(
        orders
            .iter()
            .any(|order| order.client_order_id.starts_with("alpaca-"))
    );

    let fills = runtime.recent_fills(500).expect("fills should load");
    assert!(!fills.is_empty());

    let positions = runtime
        .recent_positions(500)
        .expect("positions should load");
    assert!(
        positions
            .iter()
            .any(|position| position.reason == "order_filled")
    );
}

#[test]
fn manual_signal_generates_order_and_connector_aware_events() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

    let outcome = runtime
        .process_manual_signal(
            "aapl",
            IndicatorSignal::BuyConfirmed,
            123.45,
            chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&chrono::Utc),
        )
        .expect("manual signal processing should succeed");

    assert_eq!(outcome.signal, IndicatorSignal::BuyConfirmed);
    assert_eq!(outcome.intent, TradeIntent::OpenLong);

    let orders = runtime.recent_orders(20).expect("orders should load");
    assert!(!orders.is_empty());
    assert!(orders[0].client_order_id.starts_with("alpaca-"));

    let order_events = runtime
        .recent_events_by_scope("order", 20)
        .expect("order events should load");
    let submitted = order_events
        .iter()
        .find(|event| event.kind == "order.submitted")
        .expect("expected order.submitted event");
    let payload = serde_json::from_str::<serde_json::Value>(&submitted.payload)
        .expect("payload should be valid JSON");
    assert_eq!(payload["connector_kind"], "alpaca");
    assert!(
        payload["client_order_id"]
            .as_str()
            .expect("client_order_id should be string")
            .starts_with("alpaca-")
    );

    let provider_events = runtime
        .recent_events_by_scope("provider", 20)
        .expect("provider events should load");
    let requested = provider_events
        .iter()
        .find(|event| event.kind == "provider.order_submission.requested")
        .expect("expected provider order submission request event");
    let requested_payload = serde_json::from_str::<serde_json::Value>(&requested.payload)
        .expect("provider request payload should be valid JSON");
    assert_eq!(requested_payload["operation"], "submit_order");
    assert_eq!(requested_payload["stage"], "requested");

    let succeeded = provider_events
        .iter()
        .find(|event| event.kind == "provider.order_submission.succeeded")
        .expect("expected provider order submission success event");
    let succeeded_payload = serde_json::from_str::<serde_json::Value>(&succeeded.payload)
        .expect("provider success payload should be valid JSON");
    assert_eq!(succeeded_payload["operation"], "submit_order");
    assert_eq!(succeeded_payload["stage"], "succeeded");
    assert_eq!(succeeded_payload["connector_kind"], "alpaca");
}

#[test]
fn confirmed_manual_signal_stays_fresh_until_close_plus_grace() {
    let mut config = fixture_bundle();
    config.risk_profiles[0].stale_data_ms = 5_000;

    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

    let timeframe_ms = i64::try_from(config.instances[0].timeframe.duration().as_millis())
        .expect("timeframe should fit in i64");
    let timestamp = Utc::now() - Duration::milliseconds(timeframe_ms + 4_000);

    let outcome = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .expect("manual signal should process within stale grace");

    assert!(matches!(outcome.risk, ProcessBarRisk::Allowed));
}

#[test]
fn confirmed_manual_signal_rejects_after_close_plus_grace() {
    let mut config = fixture_bundle();
    config.risk_profiles[0].stale_data_ms = 5_000;

    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

    let timeframe_ms = i64::try_from(config.instances[0].timeframe.duration().as_millis())
        .expect("timeframe should fit in i64");
    let timestamp = Utc::now() - Duration::milliseconds(timeframe_ms + 7_000);

    let outcome = runtime
        .process_manual_signal("aapl", IndicatorSignal::BuyConfirmed, 123.45, timestamp)
        .expect("manual signal should process outside stale grace");

    assert!(matches!(
        outcome.risk,
        ProcessBarRisk::Rejected { ref reason } if reason == "stale market data"
    ));
}

#[test]
fn manual_signal_respects_bot_budget_across_multi_symbol_lanes() {
    let mut config = fixture_bundle();
    config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];
    config.instances[0].budget.pct = 50.0;

    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

    let aapl_lane = runtime
        .resolve_lane_id("aapl", "AAPL")
        .expect("AAPL lane should resolve");

    {
        let lane = runtime
            .instance_mut(&aapl_lane)
            .expect("AAPL lane runtime should exist");
        lane.has_position = true;
        lane.position_quantity = 100.0;
        lane.position_notional_usd = 10_000.0;
    }

    let outcome = runtime
        .process_manual_signal_for_symbol(
            "aapl",
            "MSFT",
            IndicatorSignal::BuyConfirmed,
            123.45,
            chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&chrono::Utc),
        )
        .expect("manual signal processing should succeed");

    assert_eq!(outcome.signal, IndicatorSignal::BuyConfirmed);
    assert_eq!(outcome.intent, TradeIntent::OpenLong);
    assert!(matches!(
        outcome.risk,
        ProcessBarRisk::Rejected {
            ref reason
        } if reason == "bot_ledger_exhausted"
    ));

    let risk_decisions = runtime
        .recent_risk_decisions(20)
        .expect("risk decisions should load");
    let msft_decision = risk_decisions
        .iter()
        .find(|decision| decision.bot_id == "aapl" && decision.symbol.as_deref() == Some("MSFT"))
        .expect("MSFT manual signal should write a risk decision");

    assert_eq!(msft_decision.decision, "rejected");
    assert_eq!(
        msft_decision.reason.as_deref(),
        Some("bot ledger exhausted")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn warmup_pending_confirmed_bars_seed_state_without_side_effects() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime
        .start_instance("aapl")
        .expect("aapl should start in fixture runtime");

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

    let signals_before = runtime
        .recent_signals(50)
        .expect("signals should load")
        .len();
    let intents_before = runtime
        .recent_intents(50)
        .expect("intents should load")
        .len();
    let risk_before = runtime
        .recent_risk_decisions(50)
        .expect("risk decisions should load")
        .len();
    let orders_before = runtime.recent_orders(50).expect("orders should load").len();

    let preview = runtime
        .process_bar(
            "aapl",
            &test_bar_at("2030-01-01T00:00:00Z", 100.0),
            SignalPhase::Preview,
        )
        .expect("preview bar should process");
    assert_eq!(preview.signal, IndicatorSignal::None);
    assert_eq!(preview.intent, TradeIntent::NoOp);

    let first_confirmed = runtime
        .process_bar(
            "aapl",
            &test_bar_at("2030-01-01T00:01:00Z", 101.0),
            SignalPhase::Confirmed,
        )
        .expect("first confirmed bar should process");
    assert_eq!(first_confirmed.signal, IndicatorSignal::None);
    assert_eq!(first_confirmed.intent, TradeIntent::NoOp);

    let second_confirmed = runtime
        .process_bar(
            "aapl",
            &test_bar_at("2030-01-01T00:02:00Z", 102.0),
            SignalPhase::Confirmed,
        )
        .expect("second confirmed bar should process");
    assert_eq!(second_confirmed.signal, IndicatorSignal::None);
    assert_eq!(second_confirmed.intent, TradeIntent::NoOp);

    let summary = runtime
        .get_instance("aapl")
        .expect("instance summary should exist");
    assert!(summary.warmup.ready);
    assert_eq!(summary.warmup.loaded_bars, 2);
    assert!(summary.warmup.last_error.is_none());

    assert_eq!(
        runtime
            .recent_signals(50)
            .expect("signals should load")
            .len(),
        signals_before
    );
    assert_eq!(
        runtime
            .recent_intents(50)
            .expect("intents should load")
            .len(),
        intents_before
    );
    assert_eq!(
        runtime
            .recent_risk_decisions(50)
            .expect("risk decisions should load")
            .len(),
        risk_before
    );
    assert_eq!(
        runtime.recent_orders(50).expect("orders should load").len(),
        orders_before
    );

    let tradable = runtime
        .process_bar(
            "aapl",
            &test_bar_at("2030-01-01T00:03:00Z", 103.0),
            SignalPhase::Confirmed,
        )
        .expect("post-warmup confirmed bar should process");
    assert_eq!(tradable.instance_id, "aapl");
    assert!(
        runtime
            .recent_intents(50)
            .expect("intents should load")
            .len()
            > intents_before
    );
    assert!(
        runtime
            .recent_risk_decisions(50)
            .expect("risk decisions should load")
            .len()
            > risk_before
    );
}
