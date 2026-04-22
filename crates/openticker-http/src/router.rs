use crate::constants::{
    BOT_CANCEL_OPEN_ORDERS_PATH, BOT_CLOSE_POSITIONS_PATH, BOT_CYCLE_DETAIL_PATH, BOT_CYCLES_PATH,
    BOT_LANES_PATH, BOT_MANUAL_SIGNAL_PATH, BOT_PATH, BOT_PAUSE_PATH, BOT_RECONCILE_PATH,
    BOT_RECONCILIATION_REPORT_PATH, BOT_RESUME_PATH, BOT_SIMULATE_BAR_PATH,
    BOT_SIMULATE_TRADE_PATH, BOT_SNAPSHOT_PATH, BOT_START_PATH, BOT_STOP_PATH, BOT_TICK_PATH,
    BOTS_PATH, CONFIG_EFFECTIVE_PATH, CONFIG_RELOAD_PATH, CONNECTORS_MATRIX_PATH,
    CONNECTORS_STATUS_PATH, DASHBOARD_ACTIVITY_PATH, DASHBOARD_BOT_DETAIL_PATH,
    DASHBOARD_BOTS_PATH, DASHBOARD_CONFIG_PATH, DASHBOARD_CONNECTORS_PATH,
    DASHBOARD_CYCLE_DETAIL_PATH, DASHBOARD_CYCLES_PATH, DASHBOARD_FEED_DETAIL_PATH,
    DASHBOARD_FEEDS_PATH, DASHBOARD_LEDGER_PATH, DASHBOARD_PATH, DASHBOARD_PORTFOLIO_PATH,
    DASHBOARD_PROVIDERS_PATH, DASHBOARD_SNAPSHOT_PATH, DATA_STREAMS_PATH, EVENTS_PATH, FILLS_PATH,
    HEALTH_PATH, INTENTS_PATH, LEDGER_ACCOUNTS_PATH, LEDGER_BOTS_PATH, LEDGER_LANES_PATH,
    LEDGER_PATH, METRICS_PATH, OPENAPI_PATH, ORDERS_PATH, POSITIONS_PATH, READY_PATH,
    RECONCILIATIONS_PATH, RISK_DECISIONS_PATH, SERVICE_STATUS_PATH, SIGNALS_PATH,
};
use crate::handlers::{
    bot_reconciliation_report_handler, bot_snapshot_handler, cancel_bot_open_orders_handler,
    close_bot_positions_handler, config_effective_handler, config_reload_handler,
    connectors_matrix_handler, connectors_status_handler, dashboard_activity_handler,
    dashboard_bots_handler, dashboard_config_handler, dashboard_connectors_handler,
    dashboard_cycle_detail_handler, dashboard_cycles_handler, dashboard_feeds_handler,
    dashboard_handler, dashboard_portfolio_handler, dashboard_providers_handler,
    dashboard_snapshot_handler, disable_kill_switch_handler, enable_kill_switch_handler,
    get_bot_cycle_handler, get_bot_handler, get_bot_lanes_handler, health_handler,
    ledger_accounts_handler, ledger_bots_handler, ledger_handler, ledger_lanes_handler,
    list_bot_cycles_handler, list_bots_handler, list_data_stream_bars_handler,
    list_data_stream_history_handler, list_data_streams_handler, list_events_handler,
    list_fills_handler, list_intents_handler, list_orders_handler, list_positions_handler,
    list_reconciliations_handler, list_risk_decisions_handler, list_signals_handler,
    manual_bot_signal_handler, metrics_handler, openapi_handler, pause_bot_handler, ready_handler,
    reconcile_bot_handler, resume_bot_handler, service_status_handler, simulate_bot_bar_handler,
    simulate_bot_trade_handler, start_bot_handler, stop_bot_handler, tick_bot_handler,
};
use crate::state::HttpState;
use axum::Router;
use axum::routing::{get, post};
use tower_http::trace::{
    DefaultMakeSpan, DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer,
};
use tracing::Level;

pub fn build_router(state: HttpState) -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route(DASHBOARD_PATH, get(dashboard_handler))
        .route(DASHBOARD_ACTIVITY_PATH, get(dashboard_activity_handler))
        .route(DASHBOARD_BOTS_PATH, get(dashboard_bots_handler))
        .route(DASHBOARD_BOT_DETAIL_PATH, get(dashboard_bots_handler))
        .route(DASHBOARD_CONFIG_PATH, get(dashboard_config_handler))
        .route(DASHBOARD_CONNECTORS_PATH, get(dashboard_connectors_handler))
        .route(DASHBOARD_CYCLES_PATH, get(dashboard_cycles_handler))
        .route(
            DASHBOARD_CYCLE_DETAIL_PATH,
            get(dashboard_cycle_detail_handler),
        )
        .route(DASHBOARD_FEEDS_PATH, get(dashboard_feeds_handler))
        .route(DASHBOARD_FEED_DETAIL_PATH, get(dashboard_feeds_handler))
        .route(DASHBOARD_PROVIDERS_PATH, get(dashboard_providers_handler))
        .route(DASHBOARD_PORTFOLIO_PATH, get(dashboard_portfolio_handler))
        .route(DASHBOARD_SNAPSHOT_PATH, get(dashboard_snapshot_handler))
        .route(HEALTH_PATH, get(health_handler))
        .route(READY_PATH, get(ready_handler))
        .route(METRICS_PATH, get(metrics_handler))
        .route(OPENAPI_PATH, get(openapi_handler))
        .route(SERVICE_STATUS_PATH, get(service_status_handler))
        .route(LEDGER_PATH, get(ledger_handler))
        .route(LEDGER_ACCOUNTS_PATH, get(ledger_accounts_handler))
        .route(LEDGER_BOTS_PATH, get(ledger_bots_handler))
        .route(LEDGER_LANES_PATH, get(ledger_lanes_handler))
        .route(DASHBOARD_LEDGER_PATH, get(ledger_handler))
        .route(CONFIG_RELOAD_PATH, post(config_reload_handler))
        .route(CONFIG_EFFECTIVE_PATH, get(config_effective_handler))
        .route(CONNECTORS_MATRIX_PATH, get(connectors_matrix_handler))
        .route(CONNECTORS_STATUS_PATH, get(connectors_status_handler))
        .route(DATA_STREAMS_PATH, get(list_data_streams_handler))
        .route(
            "/v1/data/streams/{account}/{symbol}/{timeframe}/bars",
            get(list_data_stream_bars_handler),
        )
        .route(
            "/v1/data/streams/{account}/{symbol}/{timeframe}/history",
            get(list_data_stream_history_handler),
        )
        .route(EVENTS_PATH, get(list_events_handler))
        .route(SIGNALS_PATH, get(list_signals_handler))
        .route(INTENTS_PATH, get(list_intents_handler))
        .route(RISK_DECISIONS_PATH, get(list_risk_decisions_handler))
        .route(ORDERS_PATH, get(list_orders_handler))
        .route(FILLS_PATH, get(list_fills_handler))
        .route(POSITIONS_PATH, get(list_positions_handler))
        .route(RECONCILIATIONS_PATH, get(list_reconciliations_handler))
        .route(BOTS_PATH, get(list_bots_handler))
        .route(BOT_PATH, get(get_bot_handler))
        .route(BOT_SNAPSHOT_PATH, get(bot_snapshot_handler))
        .route(BOT_LANES_PATH, get(get_bot_lanes_handler))
        .route(BOT_CYCLES_PATH, get(list_bot_cycles_handler))
        .route(BOT_CYCLE_DETAIL_PATH, get(get_bot_cycle_handler))
        .route(
            BOT_RECONCILIATION_REPORT_PATH,
            get(bot_reconciliation_report_handler),
        )
        .route(BOT_SIMULATE_BAR_PATH, post(simulate_bot_bar_handler))
        .route(BOT_SIMULATE_TRADE_PATH, post(simulate_bot_trade_handler))
        .route(BOT_MANUAL_SIGNAL_PATH, post(manual_bot_signal_handler))
        .route(BOT_TICK_PATH, post(tick_bot_handler))
        .route(BOT_START_PATH, post(start_bot_handler))
        .route(BOT_STOP_PATH, post(stop_bot_handler))
        .route(BOT_PAUSE_PATH, post(pause_bot_handler))
        .route(BOT_RESUME_PATH, post(resume_bot_handler))
        .route(BOT_RECONCILE_PATH, post(reconcile_bot_handler))
        .route(
            BOT_CANCEL_OPEN_ORDERS_PATH,
            post(cancel_bot_open_orders_handler),
        )
        .route(BOT_CLOSE_POSITIONS_PATH, post(close_bot_positions_handler))
        .route("/v1/risk/kill-switch", post(enable_kill_switch_handler))
        .route(
            "/v1/risk/clear-kill-switch",
            post(disable_kill_switch_handler),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .with_state(state)
}
