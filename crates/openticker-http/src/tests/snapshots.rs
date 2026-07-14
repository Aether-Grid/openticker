use super::*;

#[tokio::test]
async fn dashboard_snapshot_endpoint_returns_aggregated_home_payloads() {
    let app = build_router(fixture_state());
    start_instance(&app, "aapl").await;
    replay_bars_for_instance(&app, "aapl").await;

    let snapshot = get_json(&app, "/v1/dashboard/snapshot?limit=20").await;
    assert_eq!(snapshot["status"]["total_instances"], 1);
    assert_eq!(snapshot["instances"].as_array().map(Vec::len), Some(1));
    assert_eq!(snapshot["dataStreams"].as_array().map(Vec::len), Some(1));
    assert!(snapshot["signals"].as_array().is_some());
    assert!(snapshot["events"].as_array().is_some());
    assert!(snapshot["providerEvents"].as_array().is_some());
    assert!(snapshot["connectorsStatus"].as_array().is_some());
    assert!(snapshot["connectorsMatrix"].as_array().is_some());
    assert_eq!(
        snapshot["ledger"]["accounts"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(snapshot["risk"]["count"].is_number());
    assert!(snapshot["risk"]["items"].as_array().is_some());
}

#[tokio::test]
async fn bot_snapshot_endpoint_returns_focused_dashboard_payloads() {
    let app = build_router(fixture_state());
    start_instance(&app, "aapl").await;
    replay_bars_for_instance(&app, "aapl").await;
    cancel_all_orders_for_instance(&app, "aapl").await;
    close_all_positions_for_instance(&app, "aapl").await;

    let snapshot = get_json(&app, "/v1/bots/aapl/snapshot").await;
    assert_eq!(snapshot["detail"]["id"], "aapl");
    assert!(snapshot["detail"]["lanes"].as_array().is_some());
    let reconciliation = snapshot["detail"]["reconciliation_by_symbol"]
        .as_array()
        .expect("reconciliation_by_symbol should be an array");
    assert_eq!(reconciliation.len(), 1);
    assert_eq!(reconciliation[0]["symbol"], "AAPL");
    assert_eq!(snapshot["report"]["bot"]["id"], "aapl");
    assert!(snapshot["timeline"].as_array().is_some());
    assert!(snapshot["orders"].as_array().is_some());
    assert!(snapshot["fills"].as_array().is_some());
    assert!(snapshot["positions"].as_array().is_some());
}
