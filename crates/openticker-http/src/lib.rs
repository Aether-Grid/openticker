//! Externally-exposed Axum HTTP API and embedded dashboard server.
//!
//! # Authentication
//!
//! Bearer-token authentication is opt-in via the `OPENTICKER_API_TOKEN`
//! environment variable (see [`API_TOKEN_ENV`]), read at server startup by
//! [`serve`]. When set and non-empty, every endpoint requires
//! `Authorization: Bearer <token>` except health/readiness/metrics probes
//! and the embedded dashboard SPA assets; with a token configured, the
//! served dashboard cannot call the API itself. When unset or empty, the
//! API is unauthenticated (localhost development) and a warning is logged.

mod config_ops;
mod config_watcher;
mod config_write_handlers;
mod constants;
mod handlers;
mod router;
mod runtime;
mod state;

pub use constants::{
    BOT_CANCEL_OPEN_ORDERS_PATH, BOT_CLOSE_POSITIONS_PATH, BOT_CYCLE_DETAIL_PATH, BOT_CYCLES_PATH,
    BOT_LANES_PATH, BOT_MANUAL_SIGNAL_PATH, BOT_PATH, BOT_PAUSE_PATH, BOT_RECONCILE_PATH,
    BOT_RECONCILIATION_REPORT_PATH, BOT_RESUME_PATH, BOT_SIMULATE_BAR_PATH,
    BOT_SIMULATE_TRADE_PATH, BOT_SNAPSHOT_PATH, BOT_START_PATH, BOT_STOP_PATH, BOT_TICK_PATH,
    BOTS_PATH, CONFIG_ACCOUNT_PATH, CONFIG_BOT_PATH, CONFIG_BOTS_PATH, CONFIG_EFFECTIVE_PATH,
    CONFIG_GLOBAL_PATH, CONFIG_RELOAD_PATH, CONFIG_RELOAD_STATUS_PATH, CONFIG_RISK_PROFILE_PATH,
    CONNECTORS_MATRIX_PATH, CONNECTORS_STATUS_PATH, DASHBOARD_ACTIVITY_PATH,
    DASHBOARD_BOT_DETAIL_PATH, DASHBOARD_BOTS_PATH, DASHBOARD_CONFIG_PATH,
    DASHBOARD_CONNECTORS_PATH, DASHBOARD_CYCLES_PATH, DASHBOARD_FEED_DETAIL_PATH,
    DASHBOARD_FEEDS_PATH, DASHBOARD_LEDGER_PATH, DASHBOARD_PATH, DASHBOARD_PORTFOLIO_PATH,
    DASHBOARD_PROVIDERS_PATH, DASHBOARD_SNAPSHOT_PATH, DATA_STREAMS_PATH, EVENTS_PATH, FILLS_PATH,
    HEALTH_PATH, INTENTS_PATH, LEDGER_ACCOUNTS_PATH, LEDGER_BOTS_PATH, LEDGER_LANES_PATH,
    LEDGER_PATH, METRICS_PATH, OPENAPI_PATH, ORDERS_PATH, POSITIONS_PATH, READY_PATH,
    RECONCILIATIONS_PATH, RISK_DECISIONS_PATH, SERVICE_STATUS_PATH, SIGNALS_PATH,
};
pub use router::{API_TOKEN_ENV, build_router, build_router_with_token};
pub use runtime::{load_http_state, serve};
pub use state::{HealthResponse, HttpState, ReadyResponse};

#[cfg(test)]
use axum::http::StatusCode;
#[cfg(test)]
pub(crate) use constants::HTTP_SURFACE_ROUTES;
#[cfg(test)]
use openticker_config::load_from_dir;
#[cfg(test)]
use openticker_core::OhlcvBar;
#[cfg(test)]
use openticker_dataplane::StreamKey;
#[cfg(test)]
use openticker_runtime::Runtime;
#[cfg(test)]
use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use openticker_config::{
        AccountConfig, BudgetConfig, ConfigBundle, ExecutionConstraintsConfig, GlobalConfig,
        HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
        ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig,
        SignalMode, StorageConfig,
    };
    use openticker_core::{ExecutionMode, MarketType, Timeframe};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::util::ServiceExt;

    /// Upper bound when collecting response bodies in tests (16 MB). Kept
    /// explicit instead of `usize::MAX` so that response-size discipline is
    /// enforced if streaming or otherwise unbounded endpoints are ever added:
    /// a runaway body fails the test rather than exhausting memory.
    const MAX_TEST_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

    #[tokio::test]
    async fn dashboard_root_returns_html() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("OpenTicker"));
        assert!(html.contains("id=\"__nuxt\""));
    }

    #[tokio::test]
    async fn dashboard_path_returns_html() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(DASHBOARD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("OpenTicker"));
    }

    #[tokio::test]
    async fn dashboard_section_routes_return_html() {
        let app = build_router(fixture_state());
        let routes = [
            DASHBOARD_ACTIVITY_PATH,
            DASHBOARD_BOTS_PATH,
            "/bots/demo-bot",
            DASHBOARD_CONFIG_PATH,
            DASHBOARD_CONNECTORS_PATH,
            DASHBOARD_CYCLES_PATH,
            "/cycles/demo-bot/trc_test",
            DASHBOARD_FEEDS_PATH,
            "/feeds/alpaca/AAPL/1m",
            DASHBOARD_PROVIDERS_PATH,
            DASHBOARD_PORTFOLIO_PATH,
        ];

        for path in routes {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "path {path} did not return OK"
            );
            let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
                .await
                .unwrap();
            let html = String::from_utf8(body.to_vec()).unwrap();
            assert!(html.contains("OpenTicker"));
        }
    }

    #[tokio::test]
    async fn unknown_frontend_path_falls_back_to_spa_shell() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/some/deep/spa/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("OpenTicker"));
    }

    #[tokio::test]
    async fn nuxt_js_asset_served_with_javascript_mime() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/.output/public/_nuxt");
        let sample = std::fs::read_dir(&dir)
            .expect("ui/.output/public/_nuxt must exist — run `pnpm build` in ui/")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|name| {
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
            })
            .expect("at least one .js asset should be emitted by the Nuxt build");

        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/_nuxt/{sample}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(
            content_type.starts_with("application/javascript"),
            "expected javascript content-type for {sample}, got `{content_type}`"
        );
    }

    #[tokio::test]
    async fn missing_nuxt_asset_returns_not_found() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/_nuxt/does-not-exist.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ui_asset_path_traversal_returns_not_found() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/_nuxt/../index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    const TEST_API_TOKEN: &str = "secret-test-token";

    #[tokio::test]
    async fn api_returns_unauthorized_without_valid_token_when_auth_enabled() {
        let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(SERVICE_STATUS_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(missing.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"]
                .as_str()
                .unwrap_or_default()
                .contains("bearer token")
        );

        let wrong = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(SERVICE_STATUS_PATH)
                    .header("authorization", "Bearer not-the-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

        let blocked_post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bots/aapl/stop")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(blocked_post.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri(SERVICE_STATUS_PATH)
                    .header("authorization", format!("Bearer {TEST_API_TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_exempts_probes_and_dashboard_assets() {
        let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

        let exempt_paths = [
            HEALTH_PATH,
            READY_PATH,
            METRICS_PATH,
            "/",
            DASHBOARD_PATH,
            "/favicon.ico",
            "/some/deep/spa/route",
        ];
        for path in exempt_paths {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "path {path} should be reachable without a token"
            );
        }

        // Asset routes are exempt as well: a missing asset must be a plain
        // 404, never a 401.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/_nuxt/does-not-exist.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_open_when_token_unset_or_empty() {
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

        let app = build_router_with_token(fixture_state(), Some(String::new()));
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
    }

    #[tokio::test]
    async fn openapi_endpoint_requires_auth_when_token_is_set() {
        // /openapi.json is deliberately not in AUTH_EXEMPT_ROUTES — it exposes
        // the full API surface and should be gated behind the token.
        let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

        let unauthenticated = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(OPENAPI_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        assert!(
            unauthenticated
                .headers()
                .get(axum::http::header::WWW_AUTHENTICATE)
                .is_some(),
            "401 response must include WWW-Authenticate header"
        );

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri(OPENAPI_PATH)
                    .header("authorization", format!("Bearer {TEST_API_TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn oversized_request_body_is_rejected() {
        let app = build_router(fixture_state());
        let oversized_payload = format!(
            "{{\"padding\":\"{}\"}}",
            "x".repeat(crate::constants::MAX_REQUEST_BODY_BYTES + 1)
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bots/aapl/simulate-bar")
                    .header("content-type", "application/json")
                    .body(Body::from(oversized_payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn excessive_limit_query_parameter_is_clamped_and_succeeds() {
        let app = build_router(fixture_state());
        for uri in [
            "/v1/events?limit=999999999",
            "/v1/signals?limit=999999999",
            "/v1/orders?limit=999999999",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "uri {uri} should succeed with a clamped limit"
            );
        }
    }

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
        let payload =
            serde_json::from_str::<serde_json::Value>(submitted["payload"].as_str().unwrap())
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

    async fn start_instance(app: &axum::Router, instance_id: &str) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/bots/{instance_id}/start"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn replay_bars_for_instance(app: &axum::Router, instance_id: &str) {
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
                        .uri(format!("/v1/bots/{instance_id}/simulate-bar"))
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    async fn cancel_all_orders_for_instance(app: &axum::Router, instance_id: &str) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/bots/{instance_id}/cancel-open-orders"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn close_all_positions_for_instance(app: &axum::Router, instance_id: &str) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/bots/{instance_id}/close-positions"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
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

    async fn get_json(app: &axum::Router, path: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
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
    async fn config_effective_returns_not_implemented_without_bundle() {
        let app = build_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(CONFIG_EFFECTIVE_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn config_effective_returns_payload_with_managed_bundle() {
        let app = build_router(fixture_state_with_config());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(CONFIG_EFFECTIVE_PATH)
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
        assert!(json["bots"].is_array());
    }

    #[tokio::test]
    async fn config_effective_redacts_secret_field_names() {
        let mut bundle = fixture_bundle();
        bundle.accounts[0].api_key_env = Some("OPENTICKER_HTTP_API_KEY".to_owned());
        bundle.accounts[0].api_secret_env = Some("OPENTICKER_HTTP_API_SECRET".to_owned());
        let app = build_router(HttpState::with_config(
            Runtime::from_config(&bundle),
            PathBuf::from("./config"),
            bundle,
        ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri(CONFIG_EFFECTIVE_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let body_text = String::from_utf8(body.to_vec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&body_text).unwrap();
        assert!(json["accounts"][0]["secret_status"].is_object());
        assert!(!body_text.contains("api_key_env"));
        assert!(!body_text.contains("api_secret_env"));
        assert!(!body_text.contains("OPENTICKER_HTTP_API_KEY"));
        assert!(!body_text.contains("OPENTICKER_HTTP_API_SECRET"));
    }

    #[tokio::test]
    async fn config_reload_succeeds_for_valid_managed_config() {
        let config_dir = create_managed_config_dir("reload-valid");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir, bundle));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_reload_applies_polling_interval_change() {
        let config_dir = create_managed_config_dir("reload-polling-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

        let reload_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reload_response.status(), StatusCode::OK);

        let instance_response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/bots/aapl")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(instance_response.status(), StatusCode::OK);
        let body = to_bytes(instance_response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["polling"]["enabled"], true);
        assert_eq!(json["polling"]["interval_ms"], 250);
        assert_eq!(json["position"]["has_position"], false);
        assert_eq!(json["position"]["quantity"], 0.0);
        assert!(json["position"]["entry_price"].is_null());
    }

    #[tokio::test]
    async fn config_reload_rejects_invalid_connector_binding() {
        let config_dir = create_managed_config_dir("reload-invalid");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_config(&config_dir, &storage_path, "kraken", "1m");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("execution_connector"));
    }

    #[tokio::test]
    async fn config_reload_rejects_storage_path_change() {
        let config_dir = create_managed_config_dir("reload-storage-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        let changed_storage_path = config_dir.join("runtime-next.db");
        write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("storage"));
        assert_eq!(json["violations"][0]["code"], "storage_changed");
        assert_eq!(json["violations"][0]["scope"], "global");
    }

    #[tokio::test]
    async fn config_reload_rejects_account_credential_reference_change() {
        let config_dir = create_managed_config_dir("reload-credential-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        let alternate_secret_env = existing_env_var_name_except("PATH");
        write_managed_account(&config_dir, "PATH", alternate_secret_env);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("credential references"));
        assert_eq!(json["violations"][0]["code"], "credentials_changed");
        assert_eq!(json["violations"][0]["scope"], "account:alpaca-paper");
    }

    #[tokio::test]
    async fn config_reload_rejects_account_reconciliation_settings_change() {
        let config_dir = create_managed_config_dir("reload-reconcile-settings-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_account_with_reconciliation(
            &config_dir,
            "PATH",
            "PATH",
            true,
            Some("https://paper-api.alpaca.markets"),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("connector settings"));
        assert_eq!(json["violations"][0]["code"], "account_settings_changed");
    }

    #[tokio::test]
    async fn config_reload_rejects_timeframe_change_for_running_instance() {
        let config_dir = create_managed_config_dir("reload-running-timeframe-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        runtime.start_instance("aapl").unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_config(&config_dir, &storage_path, "alpaca", "5m");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("timeframe"));
        assert_eq!(json["violations"][0]["code"], "timeframe_changed_running");
        assert_eq!(json["violations"][0]["scope"], "bot:aapl");
    }

    #[tokio::test]
    async fn config_reload_rejects_symbol_change_for_running_instance() {
        let config_dir = create_managed_config_dir("reload-running-symbol-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        runtime.start_instance("aapl").unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_instance_with_symbol(&config_dir, "MSFT", "alpaca", "1m", None, None);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("symbols changed"));
        assert_eq!(json["violations"][0]["code"], "symbols_changed_running");
    }

    #[tokio::test]
    async fn config_reload_rejects_removing_running_instance() {
        let config_dir = create_managed_config_dir("reload-running-instance-removed");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        runtime.start_instance("aapl").unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        fs::remove_file(config_dir.join("bots").join("aapl.toml"))
            .expect("managed instance should be removed");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error = json["error"].as_str().unwrap_or_default();
        assert!(error.contains("running instance `aapl` was removed"));
        assert_eq!(json["violations"][0]["code"], "running_instance_removed");
    }

    #[tokio::test]
    async fn config_reload_status_starts_with_generation_zero_and_empty_history() {
        let app = build_router(fixture_state());

        let json = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(json["generation"], 0);
        assert!(json["last"].is_null());
        assert_eq!(json["history"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn config_reload_status_tracks_reloaded_and_no_change_outcomes() {
        let config_dir = create_managed_config_dir("reload-status-track");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "reloaded");
        assert_eq!(body["reloaded"], true);

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(reload_status["generation"], 1);
        assert_eq!(reload_status["last"]["outcome"], "reloaded");
        assert_eq!(reload_status["last"]["trigger"], "manual_api");
        assert_eq!(reload_status["last"]["generation"], 1);
        assert!(reload_status["last"]["error"].is_null());

        let (second_status, second_body) = post_reload(&app).await;
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(second_body["status"], "no_change");
        assert_eq!(second_body["reloaded"], false);

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(reload_status["generation"], 1);
        assert_eq!(reload_status["last"]["outcome"], "no_change");
        assert_eq!(reload_status["last"]["trigger"], "manual_api");
        assert_eq!(reload_status["last"]["generation"], 1);
        assert_eq!(reload_status["history"].as_array().map(Vec::len), Some(2));
        assert_eq!(reload_status["history"][0]["outcome"], "no_change");
        assert_eq!(reload_status["history"][1]["outcome"], "reloaded");
    }

    #[tokio::test]
    async fn config_reload_rejection_records_structured_violations_in_status() {
        let config_dir = create_managed_config_dir("reload-status-rejected");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        let changed_storage_path = config_dir.join("runtime-next.db");
        write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");

        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(violation_codes(&body).contains(&"storage_changed".to_owned()));

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(reload_status["generation"], 0);
        assert_eq!(reload_status["last"]["outcome"], "rejected");
        assert_eq!(reload_status["last"]["trigger"], "manual_api");
        assert_eq!(
            reload_status["last"]["violations"][0]["code"],
            "storage_changed"
        );
    }

    #[tokio::test]
    async fn config_reload_collects_all_change_set_violations() {
        let config_dir = create_managed_config_dir("reload-multi-violation");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        let changed_storage_path = config_dir.join("runtime-next.db");
        write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");
        let alternate_secret_env = existing_env_var_name_except("PATH");
        write_managed_account(&config_dir, "PATH", alternate_secret_env);

        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let codes = violation_codes(&body);
        assert!(codes.contains(&"storage_changed".to_owned()), "{codes:?}");
        assert!(
            codes.contains(&"credentials_changed".to_owned()),
            "{codes:?}"
        );
    }

    #[tokio::test]
    async fn config_reload_rejects_bot_dir_change() {
        let config_dir = create_managed_config_dir("reload-bot-dir-change");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        // Same instance files in a different directory; only service.bot_dir changes.
        fs::create_dir_all(config_dir.join("bots-next")).expect("bots-next dir should be created");
        fs::copy(
            config_dir.join("bots").join("aapl.toml"),
            config_dir.join("bots-next").join("aapl.toml"),
        )
        .expect("instance config should be copied");
        write_managed_global(&config_dir, &storage_path, "./bots-next");

        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["violations"][0]["code"], "bot_dir_changed");
        assert_eq!(body["violations"][0]["scope"], "global");
    }

    #[tokio::test]
    async fn config_reload_status_history_truncates_to_limit_newest_first() {
        let config_dir = create_managed_config_dir("reload-status-cap");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        let limit = crate::config_ops::RELOAD_STATUS_HISTORY_LIMIT;
        let cycles = u64::try_from(limit).unwrap() + 5;
        for cycle in 0..cycles {
            write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(200 + cycle));
            let (status, body) = post_reload(&app).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                body["reloaded"], true,
                "cycle {cycle} should apply a changed config"
            );
        }

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(reload_status["generation"], cycles);
        let history = reload_status["history"].as_array().unwrap();
        assert_eq!(history.len(), limit);
        assert_eq!(history[0]["generation"], cycles);
        assert_eq!(
            history[limit - 1]["generation"],
            cycles - u64::try_from(limit).unwrap() + 1
        );
        assert!(history.iter().all(|entry| entry["outcome"] == "reloaded"));
    }

    #[tokio::test]
    async fn config_reload_records_failed_outcome_for_corrupt_toml() {
        let config_dir = create_managed_config_dir("reload-corrupt-toml");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        fs::write(
            config_dir.join("openticker.toml"),
            "this is not [valid toml",
        )
        .expect("corrupt global config should be written");

        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("config reload failed")
        );

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        assert_eq!(reload_status["generation"], 0);
        assert_eq!(reload_status["last"]["outcome"], "failed");
        assert_eq!(reload_status["last"]["trigger"], "manual_api");
        assert!(reload_status["last"]["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn concurrent_config_reloads_keep_generation_and_history_coherent() {
        let config_dir = create_managed_config_dir("reload-concurrent");
        let storage_path = config_dir.join("runtime.db");
        write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

        let bundle = load_from_dir(&config_dir).unwrap();
        let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
        let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

        write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

        let responses = tokio::join!(
            post_reload(&app),
            post_reload(&app),
            post_reload(&app),
            post_reload(&app)
        );
        let responses = [responses.0, responses.1, responses.2, responses.3];

        let mut applied_count = 0u64;
        for (status, body) in &responses {
            assert!(status.is_success(), "concurrent reload returned {status}");
            if body["reloaded"] == true {
                applied_count += 1;
            }
        }
        assert!(applied_count >= 1);

        let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
        let final_generation = reload_status["generation"].as_u64().unwrap();
        assert_eq!(final_generation, applied_count);

        let history = reload_status["history"].as_array().unwrap();
        assert_eq!(history.len(), responses.len());
        let reloaded_entries = history
            .iter()
            .filter(|entry| entry["outcome"] == "reloaded")
            .count();
        assert_eq!(u64::try_from(reloaded_entries).unwrap(), applied_count);
        assert!(
            reloaded_entries <= 1,
            "a single distinct config state should apply at most once"
        );
        assert!(
            history
                .iter()
                .all(|entry| entry["generation"].as_u64().unwrap() <= final_generation)
        );
    }

    async fn post_reload(app: &axum::Router) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(CONFIG_RELOAD_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let json = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    fn violation_codes(body: &serde_json::Value) -> Vec<String> {
        body["violations"]
            .as_array()
            .map(|violations| {
                violations
                    .iter()
                    .filter_map(|violation| violation["code"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
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

    fn fixture_state() -> HttpState {
        let bundle = fixture_bundle();
        HttpState::new(Runtime::from_config(&bundle))
    }

    fn fixture_state_with_cycle_trace() -> HttpState {
        let bundle = fixture_bundle();
        let mut runtime = Runtime::from_config(&bundle);
        runtime
            .start_instance("aapl")
            .expect("fixture instance should start");
        runtime
            .process_manual_signal(
                "aapl",
                openticker_core::IndicatorSignal::BuyConfirmed,
                123.45,
                chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                    .expect("timestamp should parse")
                    .with_timezone(&chrono::Utc),
            )
            .expect("fixture signal should persist a cycle trace");
        HttpState::new(runtime)
    }

    fn fixture_state_with_config() -> HttpState {
        let bundle = fixture_bundle();
        HttpState::with_config(
            Runtime::from_config(&bundle),
            PathBuf::from("./config"),
            bundle,
        )
    }

    fn fixture_bundle() -> ConfigBundle {
        ConfigBundle {
            global: GlobalConfig {
                service: ServiceConfig {
                    environment: "test".to_owned(),
                    data_dir: "./var".into(),
                    bot_dir: "./config/bots".into(),
                },
                http: HttpConfig {
                    enabled: true,
                    bind: "127.0.0.1:8080".to_owned(),
                    request_log: true,
                    openapi_enabled: true,
                    openapi_path: "/openapi.json".to_owned(),
                },
                storage: StorageConfig {
                    kind: "sqlite".to_owned(),
                    path: "./var/openticker.db".into(),
                    busy_timeout_ms: 5_000,
                    prune_removed_bots_on_startup: false,
                },
                observability: ObservabilityConfig {
                    log_level: "info".to_owned(),
                    metrics_enabled: true,
                    metrics_path: "/metrics".to_owned(),
                },
                safety: SafetyConfig {
                    require_explicit_live_enable: true,
                    default_start_paused_if_recovery_uncertain: true,
                },
                data_plane: openticker_config::DataPlaneConfig::default(),
            },
            accounts: vec![AccountConfig {
                id: "alpaca-paper".to_owned(),
                kind: "alpaca".to_owned(),
                mode: ExecutionMode::Paper,
                api_key_env: None,
                api_secret_env: None,
                passphrase_env: None,
                use_demo_mode: false,
                reconciliation_remote_snapshot: false,
                execution_remote_submission: None,
                reconciliation_base_url: None,
                cash_balance_assets: Vec::new(),
                total_budget_usd: 20_000.0,
            }],
            risk_profiles: vec![RiskProfileConfig {
                id: "equities-default".to_owned(),
                max_daily_loss_pct: 2.0,
                max_open_positions: 2,
                target_order_notional_usd: Some(1_000.0),
                max_order_notional_usd: 1_000.0,
                max_spread_bps: 20,
                max_slippage_bps: 30,
                stale_data_ms: 3_000,
                cooldown_after_reject_ms: 1_000,
            }],
            instances: vec![InstanceConfig {
                id: "aapl".to_owned(),
                enabled: true,
                market: MarketType::Equities,
                symbols: vec!["AAPL".to_owned()],
                timeframe: Timeframe::M1,
                account: "alpaca-paper".to_owned(),
                data_connector: "alpaca".to_owned(),
                execution_connector: "alpaca".to_owned(),
                strategy: "single_indicator_signal".to_owned(),
                signal_mode: SignalMode::ConfirmedOnly,
                polling_enabled: true,
                polling_interval_ms: 1_000,
                indicators: vec![IndicatorInstanceConfig {
                    id: "trend-1".to_owned(),
                    indicator_type: "sma_crossover".to_owned(),
                    enabled: true,
                    role: None,
                    signal_policy: None,
                    weight: None,
                    metadata_filters: openticker_core::IndicatorSignalMetadataFilters::default(),
                    params: toml::Table::new(),
                }],
                execution_constraints: ExecutionConstraintsConfig::default(),
                budget: BudgetConfig { pct: 100.0 },
                risk: InstanceRiskConfig {
                    profile: "equities-default".to_owned(),
                    overrides: RiskOverrides::default(),
                },
                warmup_target_bars: None,
                allow_live: false,
            }],
        }
    }

    fn create_managed_config_dir(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openticker-http-{prefix}-{timestamp}"));
        fs::create_dir_all(root.join("accounts")).expect("accounts dir should be created");
        fs::create_dir_all(root.join("risk")).expect("risk dir should be created");
        fs::create_dir_all(root.join("bots")).expect("instances dir should be created");
        root
    }

    fn write_managed_global(config_dir: &Path, storage_path: &Path, bot_dir: &str) {
        fs::write(
            config_dir.join("openticker.toml"),
            format!(
                "[service]\nenvironment = \"test\"\ndata_dir = \"./var\"\nbot_dir = \"{bot_dir}\"\n\n[http]\nenabled = true\nbind = \"127.0.0.1:8080\"\nrequest_log = true\nopenapi_enabled = true\nopenapi_path = \"/openapi.json\"\n\n[storage]\nkind = \"sqlite\"\npath = \"{}\"\nbusy_timeout_ms = 5000\n\n[observability]\nlog_level = \"info\"\nmetrics_enabled = true\nmetrics_path = \"/metrics\"\n\n[safety]\nrequire_explicit_live_enable = true\ndefault_start_paused_if_recovery_uncertain = true\n",
                storage_path.display()
            ),
        )
        .expect("global config should be written");
    }

    fn write_managed_config(
        config_dir: &Path,
        storage_path: &Path,
        execution_connector: &str,
        timeframe: &str,
    ) {
        write_managed_global(config_dir, storage_path, "./bots");

        fs::write(
            config_dir.join("accounts").join("alpaca-paper.toml"),
            "id = \"alpaca-paper\"\nkind = \"alpaca\"\nmode = \"paper\"\napi_key_env = \"PATH\"\napi_secret_env = \"PATH\"\ntotal_budget_usd = 20000.0\n",
        )
        .expect("account config should be written");

        fs::write(
            config_dir.join("risk").join("equities-default.toml"),
            "id = \"equities-default\"\nmax_daily_loss_pct = 2.0\nmax_open_positions = 2\ntarget_order_notional_usd = 1000.0\nmax_order_notional_usd = 1000.0\nmax_spread_bps = 20\nmax_slippage_bps = 30\nstale_data_ms = 3000\ncooldown_after_reject_ms = 1000\n",
        )
        .expect("risk config should be written");

        write_managed_instance(config_dir, execution_connector, timeframe, None, None);
    }

    fn write_managed_instance(
        config_dir: &Path,
        execution_connector: &str,
        timeframe: &str,
        polling_enabled: Option<bool>,
        polling_interval_ms: Option<u64>,
    ) {
        write_managed_instance_with_symbol(
            config_dir,
            "AAPL",
            execution_connector,
            timeframe,
            polling_enabled,
            polling_interval_ms,
        );
    }

    fn write_managed_instance_with_symbol(
        config_dir: &Path,
        symbol: &str,
        execution_connector: &str,
        timeframe: &str,
        polling_enabled: Option<bool>,
        polling_interval_ms: Option<u64>,
    ) {
        let polling_enabled_line = polling_enabled
            .map(|enabled| format!("polling_enabled = {enabled}\n"))
            .unwrap_or_default();
        let polling_interval_line = polling_interval_ms
            .map(|interval| format!("polling_interval_ms = {interval}\n"))
            .unwrap_or_default();
        fs::write(
            config_dir.join("bots").join("aapl.toml"),
            format!(
                "id = \"aapl\"\nenabled = true\nmarket = \"equities\"\nsymbols = [\"{symbol}\"]\ntimeframe = \"{timeframe}\"\naccount = \"alpaca-paper\"\ndata_connector = \"alpaca\"\nexecution_connector = \"{execution_connector}\"\nstrategy = \"single_indicator_signal\"\nsignal_mode = \"confirmed_only\"\n{polling_enabled_line}{polling_interval_line}\n[[indicators]]\nid = \"trend-1\"\ntype = \"sma_crossover\"\nsignal_policy = \"confirmed_required\"\n\n[indicators.params]\nfast_length = 10\nslow_length = 30\n\n[budget]\npct = 100.0\n\n[risk]\nprofile = \"equities-default\"\n"
            ),
        )
        .expect("instance config should be written");
    }

    fn write_managed_account(config_dir: &Path, api_key_env: &str, api_secret_env: &str) {
        write_managed_account_with_reconciliation(
            config_dir,
            api_key_env,
            api_secret_env,
            false,
            None,
        );
    }

    fn write_managed_account_with_reconciliation(
        config_dir: &Path,
        api_key_env: &str,
        api_secret_env: &str,
        reconciliation_remote_snapshot: bool,
        reconciliation_base_url: Option<&str>,
    ) {
        let remote_line = if reconciliation_remote_snapshot {
            "reconciliation_remote_snapshot = true\n"
        } else {
            ""
        };
        let base_url_line = reconciliation_base_url
            .map(|base_url| format!("reconciliation_base_url = \"{base_url}\"\n"))
            .unwrap_or_default();
        fs::write(
            config_dir.join("accounts").join("alpaca-paper.toml"),
            format!(
                "id = \"alpaca-paper\"\nkind = \"alpaca\"\nmode = \"paper\"\napi_key_env = \"{api_key_env}\"\napi_secret_env = \"{api_secret_env}\"\ntotal_budget_usd = 20000.0\n{remote_line}{base_url_line}"
            ),
        )
        .expect("account config should be written");
    }

    fn test_bar_at(timestamp: &str, close: f64) -> OhlcvBar {
        serde_json::from_value(json!({
            "timestamp": timestamp,
            "open": close,
            "high": close + 0.9,
            "low": close - 0.9,
            "close": close,
            "volume": 1000.0
        }))
        .unwrap()
    }

    fn existing_env_var_name_except(exclude: &str) -> &'static str {
        ["HOME", "USER", "SHELL", "TMPDIR", "PATH"]
            .into_iter()
            .find(|candidate| *candidate != exclude && std::env::var(candidate).is_ok())
            .expect("test environment should expose at least one alternate env var")
    }

    fn replay_closes() -> Vec<f64> {
        let mut closes = Vec::new();
        let mut close = 125.0;
        for _ in 0..20 {
            close -= 1.4;
            closes.push(close);
        }
        for _ in 0..20 {
            close += 2.3;
            closes.push(close);
        }
        for _ in 0..20 {
            close -= 2.6;
            closes.push(close);
        }
        closes
    }
}
