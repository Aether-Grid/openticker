use super::*;

#[tokio::test]
async fn data_streams_endpoint_lists_registered_streams() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri(DATA_STREAMS_PATH)
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
    let streams = json.as_array().expect("streams payload should be an array");
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["key"]["symbol"], "AAPL");
    assert_eq!(streams[0]["key"]["timeframe"], "1m");
}

#[tokio::test]
async fn data_stream_bars_endpoint_returns_buffer_snapshot() {
    let state = fixture_state();
    let stream_key = StreamKey {
        account_id: "alpaca-paper".to_owned(),
        symbol: "AAPL".to_owned(),
        timeframe: Timeframe::M1,
    };
    let _ = state.data_plane.take_due_streams(1_000);
    state
        .data_plane
        .record_fetched_bar(
            &stream_key,
            1_000,
            test_bar_at("2030-01-01T00:00:00Z", 100.0),
        )
        .unwrap();
    state
        .data_plane
        .record_fetched_bar(
            &stream_key,
            2_000,
            test_bar_at("2030-01-01T00:01:00Z", 101.0),
        )
        .unwrap();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/data/streams/alpaca-paper/AAPL/1m/bars?limit=2")
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
    let bars = json.as_array().expect("bars payload should be an array");
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[1]["close"], 101.0);
}

#[tokio::test]
async fn data_stream_history_endpoint_returns_connector_history() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/data/streams/alpaca-paper/AAPL/1m/history?limit=5")
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
    assert_eq!(json["source"], "connector_history");
    let bars = json["bars"]
        .as_array()
        .expect("history payload should include a bars array");
    assert!(bars.len() <= 5);
}

#[tokio::test]
async fn data_stream_history_endpoint_returns_not_found_for_unknown_stream() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/data/streams/alpaca-paper/MSFT/1m/history?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("is not registered")
    );
}

#[tokio::test]
async fn data_stream_history_endpoint_rejects_invalid_timeframe() {
    let app = build_router(fixture_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/data/streams/alpaca-paper/AAPL/not-a-timeframe/history?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("timeframe")
    );
}
