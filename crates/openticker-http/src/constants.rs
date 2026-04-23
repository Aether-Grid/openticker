use include_dir::{Dir, include_dir};
use serde_json::json;
use std::sync::OnceLock;

pub const HEALTH_PATH: &str = "/healthz";
pub const READY_PATH: &str = "/readyz";
pub const METRICS_PATH: &str = "/metrics";
pub const OPENAPI_PATH: &str = "/openapi.json";
pub const DASHBOARD_PATH: &str = "/dashboard";
pub const DASHBOARD_ACTIVITY_PATH: &str = "/activity";
pub const DASHBOARD_BOTS_PATH: &str = "/bots";
pub const DASHBOARD_BOT_DETAIL_PATH: &str = "/bots/{id}";
pub const DASHBOARD_CONFIG_PATH: &str = "/config";
pub const DASHBOARD_CONNECTORS_PATH: &str = "/connectors";
pub const DASHBOARD_CYCLES_PATH: &str = "/cycles";
pub const DASHBOARD_CYCLE_DETAIL_PATH: &str = "/cycles/{bot_id}/{trace_id}";
pub const DASHBOARD_FEEDS_PATH: &str = "/feeds";
pub const DASHBOARD_FEED_DETAIL_PATH: &str = "/feeds/{account}/{symbol}/{timeframe}";
pub const DASHBOARD_PROVIDERS_PATH: &str = "/providers";
pub const DASHBOARD_PORTFOLIO_PATH: &str = "/portfolio";
pub const DASHBOARD_SNAPSHOT_PATH: &str = "/v1/dashboard/snapshot";
pub const SERVICE_STATUS_PATH: &str = "/v1/service/status";
pub const LEDGER_PATH: &str = "/v1/ledger";
pub const LEDGER_ACCOUNTS_PATH: &str = "/v1/ledger/accounts";
pub const LEDGER_BOTS_PATH: &str = "/v1/ledger/bots";
pub const LEDGER_LANES_PATH: &str = "/v1/ledger/lanes";
pub const DASHBOARD_LEDGER_PATH: &str = "/api/ledger";
pub const CONFIG_RELOAD_PATH: &str = "/v1/config/reload";
pub const CONFIG_EFFECTIVE_PATH: &str = "/v1/config/effective";
pub const CONNECTORS_MATRIX_PATH: &str = "/v1/connectors/matrix";
pub const CONNECTORS_STATUS_PATH: &str = "/v1/connectors/status";
pub const EVENTS_PATH: &str = "/v1/events";
pub const SIGNALS_PATH: &str = "/v1/signals";
pub const INTENTS_PATH: &str = "/v1/intents";
pub const RISK_DECISIONS_PATH: &str = "/v1/risk-decisions";
pub const ORDERS_PATH: &str = "/v1/orders";
pub const FILLS_PATH: &str = "/v1/fills";
pub const POSITIONS_PATH: &str = "/v1/positions";
pub const RECONCILIATIONS_PATH: &str = "/v1/reconciliations";
pub const DATA_STREAMS_PATH: &str = "/v1/data/streams";
pub const BOTS_PATH: &str = "/v1/bots";
pub const BOT_PATH: &str = "/v1/bots/{id}";
pub const BOT_LANES_PATH: &str = "/v1/bots/{id}/lanes";
pub const BOT_CYCLES_PATH: &str = "/v1/bots/{id}/cycles";
pub const BOT_CYCLE_DETAIL_PATH: &str = "/v1/bots/{id}/cycles/{trace_id}";
pub const BOT_SNAPSHOT_PATH: &str = "/v1/bots/{id}/snapshot";
pub const BOT_RECONCILIATION_REPORT_PATH: &str = "/v1/bots/{id}/reconciliation-report";
pub const BOT_SIMULATE_BAR_PATH: &str = "/v1/bots/{id}/simulate-bar";
pub const BOT_SIMULATE_TRADE_PATH: &str = "/v1/bots/{id}/simulate-trade";
pub const BOT_MANUAL_SIGNAL_PATH: &str = "/v1/bots/{id}/manual-signal";
pub const BOT_TICK_PATH: &str = "/v1/bots/{id}/tick";
pub const BOT_START_PATH: &str = "/v1/bots/{id}/start";
pub const BOT_STOP_PATH: &str = "/v1/bots/{id}/stop";
pub const BOT_PAUSE_PATH: &str = "/v1/bots/{id}/pause";
pub const BOT_RESUME_PATH: &str = "/v1/bots/{id}/resume";
pub const BOT_RECONCILE_PATH: &str = "/v1/bots/{id}/reconcile";
pub const BOT_CANCEL_OPEN_ORDERS_PATH: &str = "/v1/bots/{id}/cancel-open-orders";
pub const BOT_CLOSE_POSITIONS_PATH: &str = "/v1/bots/{id}/close-positions";

pub(crate) static UI_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui/.output/public");
pub(crate) const DASHBOARD_HTML: &str = include_str!("../ui/.output/public/index.html");

pub(crate) const STREAM_SPARKLINE_LIMIT: usize = 30;
pub(crate) const DEFAULT_STREAM_BARS_LIMIT: usize = 100;
pub(crate) const DASHBOARD_SNAPSHOT_DEFAULT_LIMIT: usize = 60;
pub(crate) const BOT_SNAPSHOT_TIMELINE_LIMIT: usize = 80;
pub(crate) const BOT_SNAPSHOT_ORDERS_LIMIT: usize = 500;
pub(crate) const BOT_SNAPSHOT_POSITIONS_LIMIT: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HttpRouteDescriptor {
    pub(crate) path: &'static str,
    pub(crate) method: &'static str,
    pub(crate) operation_id: &'static str,
}

pub(crate) const HTTP_SURFACE_ROUTES: &[HttpRouteDescriptor] = &[
    HttpRouteDescriptor {
        path: "/",
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_ACTIVITY_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_BOTS_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_BOT_DETAIL_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_CONFIG_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_CONNECTORS_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_CYCLES_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_CYCLE_DETAIL_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_FEEDS_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_FEED_DETAIL_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_PROVIDERS_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_PORTFOLIO_PATH,
        method: "get",
        operation_id: "dashboard_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_SNAPSHOT_PATH,
        method: "get",
        operation_id: "dashboard_snapshot_handler",
    },
    HttpRouteDescriptor {
        path: HEALTH_PATH,
        method: "get",
        operation_id: "health_handler",
    },
    HttpRouteDescriptor {
        path: READY_PATH,
        method: "get",
        operation_id: "ready_handler",
    },
    HttpRouteDescriptor {
        path: METRICS_PATH,
        method: "get",
        operation_id: "metrics_handler",
    },
    HttpRouteDescriptor {
        path: OPENAPI_PATH,
        method: "get",
        operation_id: "openapi_handler",
    },
    HttpRouteDescriptor {
        path: SERVICE_STATUS_PATH,
        method: "get",
        operation_id: "service_status_handler",
    },
    HttpRouteDescriptor {
        path: LEDGER_PATH,
        method: "get",
        operation_id: "ledger_handler",
    },
    HttpRouteDescriptor {
        path: LEDGER_ACCOUNTS_PATH,
        method: "get",
        operation_id: "ledger_accounts_handler",
    },
    HttpRouteDescriptor {
        path: LEDGER_BOTS_PATH,
        method: "get",
        operation_id: "ledger_bots_handler",
    },
    HttpRouteDescriptor {
        path: LEDGER_LANES_PATH,
        method: "get",
        operation_id: "ledger_lanes_handler",
    },
    HttpRouteDescriptor {
        path: DASHBOARD_LEDGER_PATH,
        method: "get",
        operation_id: "ledger_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_RELOAD_PATH,
        method: "post",
        operation_id: "config_reload_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_EFFECTIVE_PATH,
        method: "get",
        operation_id: "config_effective_handler",
    },
    HttpRouteDescriptor {
        path: CONNECTORS_MATRIX_PATH,
        method: "get",
        operation_id: "connectors_matrix_handler",
    },
    HttpRouteDescriptor {
        path: CONNECTORS_STATUS_PATH,
        method: "get",
        operation_id: "connectors_status_handler",
    },
    HttpRouteDescriptor {
        path: EVENTS_PATH,
        method: "get",
        operation_id: "list_events_handler",
    },
    HttpRouteDescriptor {
        path: SIGNALS_PATH,
        method: "get",
        operation_id: "list_signals_handler",
    },
    HttpRouteDescriptor {
        path: INTENTS_PATH,
        method: "get",
        operation_id: "list_intents_handler",
    },
    HttpRouteDescriptor {
        path: RISK_DECISIONS_PATH,
        method: "get",
        operation_id: "list_risk_decisions_handler",
    },
    HttpRouteDescriptor {
        path: ORDERS_PATH,
        method: "get",
        operation_id: "list_orders_handler",
    },
    HttpRouteDescriptor {
        path: FILLS_PATH,
        method: "get",
        operation_id: "list_fills_handler",
    },
    HttpRouteDescriptor {
        path: POSITIONS_PATH,
        method: "get",
        operation_id: "list_positions_handler",
    },
    HttpRouteDescriptor {
        path: RECONCILIATIONS_PATH,
        method: "get",
        operation_id: "list_reconciliations_handler",
    },
    HttpRouteDescriptor {
        path: DATA_STREAMS_PATH,
        method: "get",
        operation_id: "list_data_streams_handler",
    },
    HttpRouteDescriptor {
        path: "/v1/data/streams/{account}/{symbol}/{timeframe}/bars",
        method: "get",
        operation_id: "list_data_stream_bars_handler",
    },
    HttpRouteDescriptor {
        path: "/v1/data/streams/{account}/{symbol}/{timeframe}/history",
        method: "get",
        operation_id: "list_data_stream_history_handler",
    },
    HttpRouteDescriptor {
        path: BOTS_PATH,
        method: "get",
        operation_id: "list_bots_handler",
    },
    HttpRouteDescriptor {
        path: BOT_PATH,
        method: "get",
        operation_id: "get_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_LANES_PATH,
        method: "get",
        operation_id: "get_bot_lanes_handler",
    },
    HttpRouteDescriptor {
        path: BOT_CYCLES_PATH,
        method: "get",
        operation_id: "list_bot_cycles_handler",
    },
    HttpRouteDescriptor {
        path: BOT_CYCLE_DETAIL_PATH,
        method: "get",
        operation_id: "get_bot_cycle_handler",
    },
    HttpRouteDescriptor {
        path: BOT_SNAPSHOT_PATH,
        method: "get",
        operation_id: "bot_snapshot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_RECONCILIATION_REPORT_PATH,
        method: "get",
        operation_id: "bot_reconciliation_report_handler",
    },
    HttpRouteDescriptor {
        path: BOT_SIMULATE_BAR_PATH,
        method: "post",
        operation_id: "simulate_bot_bar_handler",
    },
    HttpRouteDescriptor {
        path: BOT_SIMULATE_TRADE_PATH,
        method: "post",
        operation_id: "simulate_bot_trade_handler",
    },
    HttpRouteDescriptor {
        path: BOT_MANUAL_SIGNAL_PATH,
        method: "post",
        operation_id: "manual_bot_signal_handler",
    },
    HttpRouteDescriptor {
        path: BOT_TICK_PATH,
        method: "post",
        operation_id: "tick_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_START_PATH,
        method: "post",
        operation_id: "start_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_STOP_PATH,
        method: "post",
        operation_id: "stop_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_PAUSE_PATH,
        method: "post",
        operation_id: "pause_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_RESUME_PATH,
        method: "post",
        operation_id: "resume_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_RECONCILE_PATH,
        method: "post",
        operation_id: "reconcile_bot_handler",
    },
    HttpRouteDescriptor {
        path: BOT_CANCEL_OPEN_ORDERS_PATH,
        method: "post",
        operation_id: "cancel_bot_open_orders_handler",
    },
    HttpRouteDescriptor {
        path: BOT_CLOSE_POSITIONS_PATH,
        method: "post",
        operation_id: "close_bot_positions_handler",
    },
    HttpRouteDescriptor {
        path: "/v1/risk/kill-switch",
        method: "post",
        operation_id: "enable_kill_switch_handler",
    },
    HttpRouteDescriptor {
        path: "/v1/risk/clear-kill-switch",
        method: "post",
        operation_id: "disable_kill_switch_handler",
    },
];

pub(crate) fn generated_openapi_spec() -> &'static serde_json::Value {
    static OPENAPI_SPEC: OnceLock<serde_json::Value> = OnceLock::new();
    OPENAPI_SPEC.get_or_init(|| {
        let mut paths = serde_json::Map::new();
        for route in HTTP_SURFACE_ROUTES {
            let entry = paths
                .entry(route.path.to_owned())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(path_item) = entry.as_object_mut() {
                path_item.insert(
                    route.method.to_owned(),
                    json!({
                        "operationId": route.operation_id,
                        "responses": {
                            "200": {
                                "description": "Success"
                            }
                        }
                    }),
                );
            }
        }

        json!({
            "openapi": "3.0.3",
            "info": {
                "title": "OpenTicker Control API",
                "version": env!("CARGO_PKG_VERSION")
            },
            "paths": paths
        })
    })
}
