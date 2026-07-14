use super::*;

#[tokio::test]
async fn simulate_bar_endpoint_generates_signal_events() {
    let app = build_router(fixture_state());

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    for close in replay_closes() {
        let body = serde_json::to_vec(&json!({
            "bar": {
                "timestamp": "2030-01-01T00:00:00Z",
                "open": close,
                "high": close + 0.9,
                "low": close - 0.9,
                "close": close,
                "volume": 1000.0
            },
            "phase": "confirmed"
        }))
        .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bots/aapl/simulate-bar")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let signals_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/signals?limit=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signals_response.status(), StatusCode::OK);
    let body = to_bytes(signals_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = json.as_array().expect("signals should be an array");
    assert!(!events.is_empty());
    assert!(events.iter().all(|event| event.get("signal").is_some()));
}

#[tokio::test]
async fn simulate_trade_endpoint_accepts_normalized_trades() {
    let app = build_router(fixture_state());

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let body = serde_json::to_vec(&json!({
        "trade": {
            "symbol": "AAPL",
            "timestamp": "2030-01-01T00:00:05Z",
            "price": 123.45,
            "quantity": 1.0
        }
    }))
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/simulate-trade")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let payload = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let outcomes: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert!(!outcomes.as_array().unwrap().is_empty());
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn manual_signal_endpoint_generates_order_and_execution_events() {
    let app = build_router(fixture_state());

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let body = serde_json::to_vec(&json!({
        "signal": "buy_confirmed",
        "price": 123.45,
        "timestamp": "2030-01-01T00:00:00Z"
    }))
    .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/manual-signal")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let instance_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/bots/aapl")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(instance_response.status(), StatusCode::OK);
    let instance_body = to_bytes(instance_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let instance_json: serde_json::Value = serde_json::from_slice(&instance_body).unwrap();
    assert_eq!(instance_json["symbol"], "AAPL");
    assert_eq!(instance_json["symbols"], json!(["AAPL"]));
    assert_eq!(instance_json["position"]["has_position"], true);
    assert!(
        instance_json["position"]["quantity"]
            .as_f64()
            .unwrap_or(0.0)
            > 0.0
    );
    assert_eq!(
        instance_json["position"]["entry_price"].as_f64(),
        Some(123.45)
    );

    let orders_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/orders?limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(orders_response.status(), StatusCode::OK);
    let orders_body = to_bytes(orders_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let orders_json: serde_json::Value = serde_json::from_slice(&orders_body).unwrap();
    let orders = orders_json.as_array().expect("orders should be an array");
    assert!(!orders.is_empty());

    let events_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?scope=order&limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events_response.status(), StatusCode::OK);
    let events_body = to_bytes(events_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let events_json: serde_json::Value = serde_json::from_slice(&events_body).unwrap();
    let events = events_json.as_array().expect("events should be an array");
    let submitted = events
        .iter()
        .find(|event| event["kind"] == "order.submitted")
        .expect("expected order.submitted event");
    let payload = serde_json::from_str::<serde_json::Value>(submitted["payload"].as_str().unwrap())
        .expect("order event payload should be valid JSON");
    assert_eq!(payload["connector_kind"], "alpaca");
    assert!(
        payload["client_order_id"]
            .as_str()
            .unwrap()
            .starts_with("alpaca-")
    );
}

#[tokio::test]
async fn tick_endpoint_dedupes_repeated_latest_bar_fetches() {
    let app = build_router(fixture_state());

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/tick")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = to_bytes(first_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let first_outcomes: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    let first_outcome_count = first_outcomes.as_array().map_or(0, Vec::len);
    // Startup warmup can pre-load the newest confirmed bar before the first manual tick.
    // When that happens, the first `/tick` is already deduped and returns no outcomes.
    assert!(first_outcome_count <= 1);

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/tick")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_body = to_bytes(second_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let second_outcomes: serde_json::Value = serde_json::from_slice(&second_body).unwrap();
    assert!(second_outcomes.as_array().is_some_and(Vec::is_empty));
}

#[tokio::test]
async fn instances_endpoint_exposes_polling_status_after_tick() {
    let app = build_router(fixture_state());

    let start_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);

    let tick_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/tick")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_response.status(), StatusCode::OK);

    let instances_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/bots")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(instances_response.status(), StatusCode::OK);

    let body = to_bytes(instances_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let instances = json.as_array().expect("instances should be an array");
    assert_eq!(instances.len(), 1);

    let polling = &instances[0]["polling"];
    assert_eq!(instances[0]["symbol"], "AAPL");
    assert_eq!(instances[0]["symbols"], json!(["AAPL"]));
    assert_eq!(polling["enabled"], true);
    assert_eq!(polling["interval_ms"], 1_000);
    assert!(polling["last_attempt_ms"].as_i64().is_some());
    assert!(polling["last_success_ms"].as_i64().is_some());
    assert!(polling["last_error"].is_null());
    assert!(polling["last_polled_bar_timestamp"].as_str().is_some());
    assert!(polling["last_polled_bar_close"].as_f64().is_some());

    let warmup = &instances[0]["warmup"];
    assert_eq!(warmup["ready"], true);
    assert!(warmup["required_bars"].as_u64().is_some());
    assert!(warmup["loaded_bars"].as_u64().is_some());
    assert!(warmup["last_error"].is_null());

    let position = &instances[0]["position"];
    assert_eq!(position["has_position"], false);
    assert_eq!(position["quantity"], 0.0);
    assert!(position["entry_price"].is_null());
}

#[tokio::test]
async fn cycle_endpoints_return_trace_summaries_and_detail() {
    let app = build_router(fixture_state_with_cycle_trace());
    assert_cycles_endpoints(&app).await;
}

#[tokio::test]
async fn reconciliation_report_endpoint_returns_explicit_differences() {
    let app = build_router(fixture_state());

    let reconcile_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/reconcile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconcile_response.status(), StatusCode::OK);

    let report_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/bots/aapl/reconciliation-report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(report_response.status(), StatusCode::OK);
    let body = to_bytes(report_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["bot"]["id"], "aapl");
    assert_eq!(json["latest"]["safe_to_trade"], true);
    let differences = json["latest"]["differences"]
        .as_array()
        .expect("differences should be an array");
    assert!(differences.is_empty());
}

#[tokio::test]
async fn instance_start_endpoint_transitions_state() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "aapl");
    assert_eq!(json["state"], "running");
}

#[tokio::test]
async fn unknown_instance_returns_404() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/bots/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn kill_switch_enable_disable_routes_change_runtime_state() {
    let app = build_router(fixture_state());
    start_instance(&app, "aapl").await;

    let enable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/risk/kill-switch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enable_response.status(), StatusCode::OK);
    let enable_body = to_bytes(enable_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let enable_json: serde_json::Value = serde_json::from_slice(&enable_body).unwrap();
    assert_eq!(enable_json["kill_switch_active"], true);
    assert_eq!(enable_json["running_instances"], 0);

    let blocked_resume = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/resume")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_resume.status(), StatusCode::CONFLICT);
    let blocked_body = to_bytes(blocked_resume.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let blocked_json: serde_json::Value = serde_json::from_slice(&blocked_body).unwrap();
    assert!(
        blocked_json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("kill switch")
    );

    let clear_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/risk/clear-kill-switch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(clear_response.status(), StatusCode::OK);
    let clear_body = to_bytes(clear_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let clear_json: serde_json::Value = serde_json::from_slice(&clear_body).unwrap();
    assert_eq!(clear_json["kill_switch_active"], false);

    let resumed = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/resume")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resumed.status(), StatusCode::OK);
}

async fn assert_cycles_endpoints(app: &axum::Router) {
    let summaries = get_json(app, "/v1/bots/aapl/cycles?limit=10").await;
    let summaries = summaries
        .as_array()
        .expect("cycle summaries should be an array");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0]["bot_id"], "aapl");
    assert_eq!(summaries[0]["symbol"], "AAPL");
    assert!(summaries[0].get("trace_id").is_some());

    let trace_id = summaries[0]["trace_id"]
        .as_str()
        .expect("trace_id should be present");
    let detail = get_json(app, &format!("/v1/bots/aapl/cycles/{trace_id}")).await;
    assert_eq!(detail["summary"]["trace_id"], trace_id);
    assert_eq!(detail["summary"]["bot_id"], "aapl");
    assert!(detail["related_events"].as_array().is_some());
}
