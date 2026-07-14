use super::*;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(HEALTH_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    assert_eq!(body, "{\"status\":\"ok\"}");
}

#[tokio::test]
async fn ready_handler_reflects_startup_reconciliation_state() {
    let ready_state = fixture_state();
    let ready_app = build_router(ready_state.clone());

    let ready_response = ready_app
        .oneshot(
            Request::builder()
                .uri(READY_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(ready_response.status(), StatusCode::OK);
    let ready_body = to_bytes(ready_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let ready_json: serde_json::Value = serde_json::from_slice(&ready_body).unwrap();
    assert_eq!(ready_json["status"], "ready");

    let config_dir = create_managed_config_dir("ready-startup-reconciliation-blocked");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");
    write_managed_account_with_reconciliation(
        &config_dir,
        "PATH",
        "PATH",
        true,
        Some("http://127.0.0.1:1"),
    );

    let bundle = load_from_dir(&config_dir).unwrap();
    let mut seeded_runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    seeded_runtime.start_instance("aapl").unwrap();
    drop(seeded_runtime);

    let blocked_runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let blocked_app = build_router(HttpState::with_config(
        blocked_runtime,
        config_dir.clone(),
        bundle,
    ));
    let blocked_response = blocked_app
        .oneshot(
            Request::builder()
                .uri(READY_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(blocked_response.status(), StatusCode::OK);
    let blocked_body = to_bytes(blocked_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let blocked_json: serde_json::Value = serde_json::from_slice(&blocked_body).unwrap();
    assert_eq!(blocked_json["status"], "not_ready");
}

#[tokio::test]
async fn openapi_endpoint_reflects_http_surface_routes() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(OPENAPI_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let openapi: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(openapi["openapi"], "3.0.3");
    assert_eq!(openapi["info"]["version"], env!("CARGO_PKG_VERSION"));
    let paths = openapi["paths"]
        .as_object()
        .expect("openapi paths should be an object");

    for route in HTTP_SURFACE_ROUTES {
        let path_item = paths.get(route.path).unwrap_or_else(|| {
            panic!("missing path in generated openapi: {}", route.path);
        });
        let operation = path_item.get(route.method).unwrap_or_else(|| {
            panic!(
                "missing method in generated openapi: {} {}",
                route.method, route.path
            );
        });
        assert_eq!(operation["operationId"], route.operation_id);
    }
}

#[tokio::test]
async fn metrics_endpoint_exposes_observability_markers() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(METRICS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let metrics = String::from_utf8(body.to_vec()).unwrap();

    assert!(metrics.contains("openticker_risk_rejects_total"));
    assert!(metrics.contains("openticker_ledger_reserve_attempts_total"));
    assert!(metrics.contains("openticker_live_mode_active"));
    assert!(metrics.contains("openticker_connector_resilience_windows_active"));
    assert!(metrics.contains("openticker_process_bar_latency_ms_last"));
    assert!(metrics.contains("openticker_execution_submit_latency_ms_last"));
    assert!(metrics.contains("openticker_connector_resilience_window_active{"));
    assert!(metrics.contains("openticker_background_poll_cycle_latency_ms_last"));
}

#[tokio::test]
async fn service_status_endpoint_returns_expected_counts() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
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
    assert_eq!(json["total_instances"], 1);
    assert_eq!(json["running_instances"], 0);
    assert_eq!(json["warmup_ready_instances"], 1);
    assert_eq!(json["warmup_pending_instances"], 0);
    assert_eq!(json["warmup_failed_instances"], 0);
}
