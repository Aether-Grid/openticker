use super::*;

#[tokio::test]
async fn events_endpoint_returns_runtime_events() {
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=5")
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
    let events = json.as_array().expect("events should be an array");
    assert!(!events.is_empty());
}

#[tokio::test]
async fn events_endpoint_supports_scope_and_entity_filters() {
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

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?entity_id=aapl&limit=20")
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
    let events = json.as_array().expect("events should be an array");
    assert!(!events.is_empty());
    assert!(events.iter().all(|event| event["entity_id"] == "aapl"));

    let scoped_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?scope=instance&entity_id=aapl&limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(scoped_response.status(), StatusCode::OK);
    let scoped_body = to_bytes(scoped_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let scoped_json: serde_json::Value = serde_json::from_slice(&scoped_body).unwrap();
    let scoped_events = scoped_json.as_array().expect("events should be an array");
    assert!(!scoped_events.is_empty());
    assert!(
        scoped_events
            .iter()
            .all(|event| { event["scope"] == "instance" && event["entity_id"] == "aapl" })
    );
}

#[tokio::test]
async fn scoped_events_and_domain_endpoints_return_matching_events() {
    let app = build_router(fixture_state());
    start_instance(&app, "aapl").await;
    replay_bars_for_instance(&app, "aapl").await;
    cancel_all_orders_for_instance(&app, "aapl").await;
    close_all_positions_for_instance(&app, "aapl").await;

    assert_order_events_scope(&app).await;
    assert_orders_endpoint(&app).await;
    assert_intents_endpoint(&app).await;
    assert_risk_decisions_endpoint(&app).await;
    assert_fills_endpoint(&app).await;
    assert_positions_endpoint(&app).await;
}

#[tokio::test]
async fn reconciliations_endpoint_returns_latest_records() {
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/reconciliations?bot_id=aapl&limit=5")
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
    let records = json.as_array().expect("reconciliations should be an array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["bot_id"], "aapl");
}

async fn assert_order_events_scope(app: &axum::Router) {
    let order_events = get_json(app, "/v1/events?scope=order&limit=10").await;
    let order_events = order_events.as_array().unwrap();
    assert!(!order_events.is_empty());
    assert!(order_events.iter().all(|event| event["scope"] == "order"));
}

async fn assert_orders_endpoint(app: &axum::Router) {
    let orders = get_json(app, "/v1/orders?limit=10").await;
    let orders = orders.as_array().unwrap();
    assert!(!orders.is_empty());
    assert!(orders.iter().all(|order| order.get("status").is_some()));

    let filtered = get_json(app, "/v1/orders?bot_id=aapl&limit=10").await;
    let filtered = filtered.as_array().unwrap();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|order| order["bot_id"] == "aapl"));
}

async fn assert_intents_endpoint(app: &axum::Router) {
    let intents = get_json(app, "/v1/intents?limit=10").await;
    assert!(!intents.as_array().unwrap().is_empty());
}

async fn assert_risk_decisions_endpoint(app: &axum::Router) {
    let risk_decisions = get_json(app, "/v1/risk-decisions?limit=10").await;
    assert!(risk_decisions["count"].as_u64().unwrap() > 0);
    assert!(!risk_decisions["items"].as_array().unwrap().is_empty());
    assert!(risk_decisions["items"][0].get("decision").is_some());
}

async fn assert_fills_endpoint(app: &axum::Router) {
    let fills = get_json(app, "/v1/fills?limit=10").await;
    assert!(!fills.as_array().unwrap().is_empty());

    let filtered = get_json(app, "/v1/fills?bot_id=aapl&limit=10").await;
    let filtered = filtered.as_array().unwrap();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|fill| fill["bot_id"] == "aapl"));
}

async fn assert_positions_endpoint(app: &axum::Router) {
    let positions = get_json(app, "/v1/positions?limit=10").await;
    assert!(!positions.as_array().unwrap().is_empty());

    let filtered = get_json(app, "/v1/positions?bot_id=aapl&limit=10").await;
    let filtered = filtered.as_array().unwrap();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|position| position["bot_id"] == "aapl"));
}
