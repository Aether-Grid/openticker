use super::*;

#[tokio::test]
async fn ledger_endpoint_returns_account_bot_and_lane_rows() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(LEDGER_PATH)
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
    assert_eq!(json["accounts"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["bots"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["lanes"].as_array().map(Vec::len), Some(0));
    assert_eq!(json["accounts"][0]["id"], "alpaca-paper");
    assert_eq!(json["bots"][0]["id"], "aapl");
}

#[tokio::test]
async fn dashboard_ledger_endpoint_returns_account_and_bot_rows() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(DASHBOARD_LEDGER_PATH)
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
    assert_eq!(json["accounts"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["bots"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["accounts"][0]["id"], "alpaca-paper");
    assert_eq!(json["bots"][0]["id"], "aapl");
}

#[tokio::test]
async fn split_ledger_endpoints_return_expected_rows() {
    let app = build_router(fixture_state());

    let accounts = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(LEDGER_ACCOUNTS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accounts.status(), StatusCode::OK);
    let accounts_body = to_bytes(accounts.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let accounts_json: serde_json::Value = serde_json::from_slice(&accounts_body).unwrap();
    assert_eq!(accounts_json.as_array().map(Vec::len), Some(1));

    let bots = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(LEDGER_BOTS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bots.status(), StatusCode::OK);
    let bots_body = to_bytes(bots.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let bots_json: serde_json::Value = serde_json::from_slice(&bots_body).unwrap();
    assert_eq!(bots_json.as_array().map(Vec::len), Some(1));

    let lanes = app
        .oneshot(
            Request::builder()
                .uri(LEDGER_LANES_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lanes.status(), StatusCode::OK);
    let lanes_body = to_bytes(lanes.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let lanes_json: serde_json::Value = serde_json::from_slice(&lanes_body).unwrap();
    assert_eq!(lanes_json.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn connectors_matrix_endpoint_returns_descriptors() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(CONNECTORS_MATRIX_PATH)
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
    let connectors = json.as_array().unwrap();
    assert!(!connectors.is_empty());
}

#[tokio::test]
async fn connectors_status_endpoint_returns_runtime_health() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(CONNECTORS_STATUS_PATH)
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
    let statuses = json.as_array().unwrap();
    assert!(!statuses.is_empty());
    assert_eq!(statuses[0]["account_id"], "alpaca-paper");
    assert!(statuses[0]["resilience_policy"].is_object());
    assert!(statuses[0]["resilience_state"].is_object());
}
