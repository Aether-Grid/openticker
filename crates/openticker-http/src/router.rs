use crate::config_write_handlers::{
    create_config_bot_handler, delete_config_bot_handler, put_config_account_handler,
    put_config_bot_handler, put_config_global_handler, put_config_risk_profile_handler,
};
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
    LEDGER_LANES_PATH, LEDGER_PATH, MAX_REQUEST_BODY_BYTES, METRICS_PATH, OPENAPI_PATH,
    ORDERS_PATH, POSITIONS_PATH, READY_PATH, RECONCILIATIONS_PATH, RISK_DECISIONS_PATH,
    SERVICE_STATUS_PATH, SIGNALS_PATH,
};
use crate::handlers::{
    bot_reconciliation_report_handler, bot_snapshot_handler, cancel_bot_open_orders_handler,
    close_bot_positions_handler, config_effective_handler, config_reload_handler,
    config_reload_status_handler, connectors_matrix_handler, connectors_status_handler,
    dashboard_handler, dashboard_snapshot_handler, disable_kill_switch_handler,
    enable_kill_switch_handler, favicon_handler, get_bot_cycle_handler, get_bot_handler,
    get_bot_lanes_handler, health_handler, ledger_accounts_handler, ledger_bots_handler,
    ledger_handler, ledger_lanes_handler, list_bot_cycles_handler, list_bots_handler,
    list_data_stream_bars_handler, list_data_stream_history_handler, list_data_streams_handler,
    list_events_handler, list_fills_handler, list_intents_handler, list_orders_handler,
    list_positions_handler, list_reconciliations_handler, list_risk_decisions_handler,
    list_signals_handler, manual_bot_signal_handler, metrics_handler, openapi_handler,
    pause_bot_handler, ready_handler, reconcile_bot_handler, resume_bot_handler,
    service_status_handler, simulate_bot_bar_handler, simulate_bot_trade_handler,
    start_bot_handler, stop_bot_handler, tick_bot_handler, ui_asset_handler,
};
use crate::state::{ErrorResponse, HttpState};
use axum::extract::{DefaultBodyLimit, MatchedPath, Request};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tower_http::trace::{
    DefaultMakeSpan, DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer,
};
use tracing::Level;

/// Environment variable holding the bearer token that protects the API.
///
/// Read at the composition root ([`crate::serve`]). When unset or empty the
/// API is served without authentication (suitable for localhost development
/// only); when set, every endpoint except health/readiness/metrics probes and
/// the embedded dashboard SPA assets requires `Authorization: Bearer <token>`.
pub const API_TOKEN_ENV: &str = "OPENTICKER_API_TOKEN";

const UI_NUXT_ASSET_ROUTE: &str = "/_nuxt/{*path}";
const UI_FONT_ASSET_ROUTE: &str = "/_fonts/{*path}";
const FAVICON_ROUTE: &str = "/favicon.ico";

/// Route templates that stay reachable without a bearer token when
/// authentication is enabled: operational probes plus the dashboard SPA shell
/// and its static assets (HTML/JS/CSS). Everything else — including every
/// control/data API route — fails closed with 401.
const AUTH_EXEMPT_ROUTES: &[&str] = &[
    HEALTH_PATH,
    READY_PATH,
    METRICS_PATH,
    "/",
    DASHBOARD_PATH,
    DASHBOARD_ACTIVITY_PATH,
    DASHBOARD_BOTS_PATH,
    DASHBOARD_BOT_DETAIL_PATH,
    DASHBOARD_CONFIG_PATH,
    DASHBOARD_CONNECTORS_PATH,
    DASHBOARD_CYCLES_PATH,
    DASHBOARD_CYCLE_DETAIL_PATH,
    DASHBOARD_FEEDS_PATH,
    DASHBOARD_FEED_DETAIL_PATH,
    DASHBOARD_PROVIDERS_PATH,
    DASHBOARD_PORTFOLIO_PATH,
    UI_NUXT_ASSET_ROUTE,
    UI_FONT_ASSET_ROUTE,
    FAVICON_ROUTE,
];

/// Builds the HTTP router without bearer-token authentication.
///
/// Suitable for tests and localhost-only deployments. Production wiring
/// should go through [`crate::serve`], which reads [`API_TOKEN_ENV`] and
/// delegates to [`build_router_with_token`].
pub fn build_router(state: HttpState) -> Router {
    build_router_with_token(state, None)
}

/// Builds the router, optionally protecting the API with a bearer token.
///
/// When `api_token` is `Some` and non-empty, every endpoint requires
/// `Authorization: Bearer <token>` except the routes in
/// [`AUTH_EXEMPT_ROUTES`] (health/readiness/metrics probes and the embedded
/// dashboard SPA shell/assets). Note that with a token configured the served
/// dashboard cannot call the API itself — that is an accepted consequence of
/// opting in. When `api_token` is `None` or empty, behavior is unchanged and
/// every endpoint is reachable unauthenticated.
///
/// The composition root ([`crate::serve`]) sources the token from the
/// [`API_TOKEN_ENV`] environment variable.
pub fn build_router_with_token(state: HttpState, api_token: Option<String>) -> Router {
    let router = api_routes();
    let router = match api_token.filter(|token| !token.is_empty()) {
        Some(token) => {
            let token: Arc<str> = Arc::from(token);
            router.layer(middleware::from_fn(move |request: Request, next: Next| {
                let token = Arc::clone(&token);
                async move { require_bearer_token(&token, request, next).await }
            }))
        }
        None => router,
    };
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .with_state(state)
}

async fn require_bearer_token(expected_token: &str, request: Request, next: Next) -> Response {
    if is_auth_exempt(&request) {
        return next.run(request).await;
    }
    // RFC 7235: auth-scheme tokens are case-insensitive ("Bearer" == "bearer").
    // Split into the scheme word and the credential, then compare the scheme
    // case-insensitively while keeping the credential verbatim.
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, credential) = value.split_once(' ')?;
            if scheme.eq_ignore_ascii_case("bearer") {
                Some(credential)
            } else {
                None
            }
        });
    match provided {
        Some(candidate) if constant_time_eq(candidate.as_bytes(), expected_token.as_bytes()) => {
            next.run(request).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            // RFC 6750 §3: servers MUST include WWW-Authenticate on 401.
            [(
                header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"openticker\""),
            )],
            Json(ErrorResponse {
                error: "missing or invalid bearer token".to_owned(),
            }),
        )
            .into_response(),
    }
}

fn is_auth_exempt(request: &Request) -> bool {
    match request.extensions().get::<MatchedPath>() {
        // No matched route means the SPA fallback will serve the dashboard
        // shell; any future API route registered explicitly is protected by
        // default (fail closed).
        None => true,
        Some(matched) => AUTH_EXEMPT_ROUTES.contains(&matched.as_str()),
    }
}

/// Constant-time byte comparison so token checks do not leak match length
/// through timing.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.ct_eq(right).into()
}

#[allow(clippy::too_many_lines)]
fn api_routes() -> Router<HttpState> {
    Router::new()
        // Dashboard SPA shell — every frontend route serves the same index.html;
        // Vue Router takes over on the client.
        .route("/", get(dashboard_handler))
        .route(DASHBOARD_PATH, get(dashboard_handler))
        .route(DASHBOARD_ACTIVITY_PATH, get(dashboard_handler))
        .route(DASHBOARD_BOTS_PATH, get(dashboard_handler))
        .route(DASHBOARD_BOT_DETAIL_PATH, get(dashboard_handler))
        .route(DASHBOARD_CONFIG_PATH, get(dashboard_handler))
        .route(DASHBOARD_CONNECTORS_PATH, get(dashboard_handler))
        .route(DASHBOARD_CYCLES_PATH, get(dashboard_handler))
        .route(DASHBOARD_CYCLE_DETAIL_PATH, get(dashboard_handler))
        .route(DASHBOARD_FEEDS_PATH, get(dashboard_handler))
        .route(DASHBOARD_FEED_DETAIL_PATH, get(dashboard_handler))
        .route(DASHBOARD_PROVIDERS_PATH, get(dashboard_handler))
        .route(DASHBOARD_PORTFOLIO_PATH, get(dashboard_handler))
        // Static UI assets emitted by `pnpm build` into ui/.output/public.
        .route(UI_NUXT_ASSET_ROUTE, get(ui_asset_handler))
        .route(UI_FONT_ASSET_ROUTE, get(ui_asset_handler))
        .route(FAVICON_ROUTE, get(favicon_handler))
        // Fallback — treat any unknown path as an SPA route. API namespaces below
        // are matched first because they're registered explicitly.
        .fallback(get(dashboard_handler))
        // JSON / text APIs
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
        .route(CONFIG_RELOAD_STATUS_PATH, get(config_reload_status_handler))
        .route(CONFIG_EFFECTIVE_PATH, get(config_effective_handler))
        .route(CONFIG_GLOBAL_PATH, put(put_config_global_handler))
        .route(CONFIG_BOTS_PATH, post(create_config_bot_handler))
        .route(
            CONFIG_BOT_PATH,
            put(put_config_bot_handler).delete(delete_config_bot_handler),
        )
        .route(
            CONFIG_RISK_PROFILE_PATH,
            put(put_config_risk_profile_handler),
        )
        .route(CONFIG_ACCOUNT_PATH, put(put_config_account_handler))
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
}
