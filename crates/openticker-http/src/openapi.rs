use crate::constants::{
    BOT_CANCEL_OPEN_ORDERS_PATH, BOT_CLOSE_POSITIONS_PATH, BOT_CYCLE_DETAIL_PATH, BOT_CYCLES_PATH,
    BOT_LANES_PATH, BOT_MANUAL_SIGNAL_PATH, BOT_PATH, BOT_PAUSE_PATH, BOT_RECONCILE_PATH,
    BOT_RECONCILIATION_REPORT_PATH, BOT_RESUME_PATH, BOT_SIMULATE_BAR_PATH,
    BOT_SIMULATE_TRADE_PATH, BOT_SNAPSHOT_PATH, BOT_START_PATH, BOT_STOP_PATH, BOT_TICK_PATH,
    BOTS_PATH, CONFIG_ACCOUNT_PATH, CONFIG_BOT_PATH, CONFIG_BOTS_PATH, CONFIG_EFFECTIVE_PATH,
    CONFIG_GLOBAL_PATH, CONFIG_RELOAD_PATH, CONFIG_RELOAD_STATUS_PATH, CONFIG_RISK_PROFILE_PATH,
    CONNECTORS_MATRIX_PATH, CONNECTORS_STATUS_PATH, DASHBOARD_ACTIVITY_PATH,
    DASHBOARD_BOT_DETAIL_PATH, DASHBOARD_BOTS_PATH, DASHBOARD_CONFIG_PATH,
    DASHBOARD_CONNECTORS_PATH, DASHBOARD_CYCLE_DETAIL_PATH, DASHBOARD_CYCLES_PATH,
    DASHBOARD_FEED_DETAIL_PATH, DASHBOARD_FEEDS_PATH, DASHBOARD_LEDGER_PATH, DASHBOARD_PATH,
    DASHBOARD_PORTFOLIO_PATH, DASHBOARD_PROVIDERS_PATH, DASHBOARD_SNAPSHOT_PATH, DATA_STREAMS_PATH,
    EVENTS_PATH, FILLS_PATH, HEALTH_PATH, INTENTS_PATH, LEDGER_ACCOUNTS_PATH, LEDGER_BOTS_PATH,
    LEDGER_LANES_PATH, LEDGER_PATH, METRICS_PATH, OPENAPI_PATH, ORDERS_PATH, POSITIONS_PATH,
    READY_PATH, RECONCILIATIONS_PATH, RISK_DECISIONS_PATH, SERVICE_STATUS_PATH, SIGNALS_PATH,
};
use serde_json::json;
use std::sync::OnceLock;

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
        path: CONFIG_RELOAD_STATUS_PATH,
        method: "get",
        operation_id: "config_reload_status_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_EFFECTIVE_PATH,
        method: "get",
        operation_id: "config_effective_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_GLOBAL_PATH,
        method: "put",
        operation_id: "put_config_global_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_BOTS_PATH,
        method: "post",
        operation_id: "create_config_bot_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_BOT_PATH,
        method: "put",
        operation_id: "put_config_bot_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_BOT_PATH,
        method: "delete",
        operation_id: "delete_config_bot_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_RISK_PROFILE_PATH,
        method: "put",
        operation_id: "put_config_risk_profile_handler",
    },
    HttpRouteDescriptor {
        path: CONFIG_ACCOUNT_PATH,
        method: "put",
        operation_id: "put_config_account_handler",
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
