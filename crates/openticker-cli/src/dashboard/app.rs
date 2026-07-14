use anyhow::Result;
use reqwest::{Client, Method};
use std::time::{Duration, Instant};
use tracing::info;

use super::client::{api_request_json, fetch_snapshot};
use super::models::{DashboardBotSummary, DashboardSnapshot};
use crate::api::encode_path_segment;
use crate::cli::DashboardOptions;

#[derive(Debug, Clone, Copy)]
pub(super) enum BotOperation {
    Start,
    Stop,
    Pause,
    Resume,
    Tick,
    Reconcile,
    CancelOpenOrders,
    ClosePositions,
}

impl BotOperation {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Tick => "tick",
            Self::Reconcile => "reconcile",
            Self::CancelOpenOrders => "cancel-open-orders",
            Self::ClosePositions => "close-positions",
        }
    }

    fn is_journaling_only(self) -> bool {
        matches!(self, Self::CancelOpenOrders)
    }

    /// Operations that can move capital or cancel live orders and therefore
    /// require an explicit confirmation keypress before firing.
    pub(super) fn is_destructive(self) -> bool {
        matches!(self, Self::CancelOpenOrders | Self::ClosePositions)
    }

    fn path(self, bot_id: &str) -> String {
        let bot_id = encode_path_segment(bot_id);
        match self {
            Self::Start => format!("/v1/bots/{bot_id}/start"),
            Self::Stop => format!("/v1/bots/{bot_id}/stop"),
            Self::Pause => format!("/v1/bots/{bot_id}/pause"),
            Self::Resume => format!("/v1/bots/{bot_id}/resume"),
            Self::Tick => format!("/v1/bots/{bot_id}/tick"),
            Self::Reconcile => format!("/v1/bots/{bot_id}/reconcile"),
            Self::CancelOpenOrders => format!("/v1/bots/{bot_id}/cancel-open-orders"),
            Self::ClosePositions => format!("/v1/bots/{bot_id}/close-positions"),
        }
    }
}

/// A destructive action awaiting an explicit second keypress to confirm.
///
/// For bot operations we capture the *resolved bot id* at request time, not
/// just the operation: the snapshot (and the selected index it is clamped to)
/// can change under us between the two keypresses, so re-resolving by index on
/// confirm could land the destructive action on a different bot than the one
/// named in the confirmation prompt.
#[derive(Debug, Clone)]
pub(super) enum PendingConfirmation {
    BotOperation {
        operation: BotOperation,
        bot_id: String,
    },
    EngageKillSwitch,
}

#[derive(Debug)]
pub(super) struct DashboardApp {
    pub(super) api_url: String,
    pub(super) limit: usize,
    pub(super) refresh_interval: Duration,
    pub(super) selected_bot: usize,
    pub(super) snapshot: DashboardSnapshot,
    pub(super) status_message: String,
    last_refresh_at: Instant,
    pub(super) pending_confirmation: Option<PendingConfirmation>,
}

impl DashboardApp {
    pub(super) fn new(options: DashboardOptions) -> Self {
        Self {
            api_url: options.api.api_url,
            limit: options.limit.clamp(1, 200),
            refresh_interval: Duration::from_millis(options.refresh_ms.max(250)),
            selected_bot: 0,
            snapshot: DashboardSnapshot::default(),
            status_message: "starting dashboard".to_owned(),
            last_refresh_at: Instant::now(),
            pending_confirmation: None,
        }
    }

    pub(super) fn selected_bot(&self) -> Option<&DashboardBotSummary> {
        self.snapshot.bots.get(self.selected_bot)
    }

    pub(super) fn selected_bot_id(&self) -> Option<&str> {
        self.selected_bot().map(|bot| bot.id.as_str())
    }

    /// Whether a bot with the given id is present in the current snapshot.
    /// Used to verify a confirmed destructive action still targets a live bot
    /// before firing it.
    pub(super) fn has_bot(&self, bot_id: &str) -> bool {
        self.snapshot.bots.iter().any(|bot| bot.id == bot_id)
    }

    pub(super) fn select_next(&mut self) {
        if self.snapshot.bots.is_empty() {
            return;
        }
        self.selected_bot = (self.selected_bot + 1) % self.snapshot.bots.len();
    }

    pub(super) fn select_previous(&mut self) {
        if self.snapshot.bots.is_empty() {
            return;
        }
        self.selected_bot = if self.selected_bot == 0 {
            self.snapshot.bots.len() - 1
        } else {
            self.selected_bot - 1
        };
    }

    pub(super) fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
    }

    pub(super) fn should_auto_refresh(&self) -> bool {
        self.last_refresh_at.elapsed() >= self.refresh_interval
    }

    pub(super) async fn refresh(&mut self, client: &Client) -> Result<()> {
        let snapshot = fetch_snapshot(client, &self.api_url, self.limit).await?;
        self.snapshot = snapshot;
        self.selected_bot = clamp_selected_index(self.selected_bot, self.snapshot.bots.len());
        self.last_refresh_at = Instant::now();
        Ok(())
    }

    pub(super) async fn execute_bot_operation(
        &mut self,
        client: &Client,
        operation: BotOperation,
        bot_id: &str,
    ) -> Result<()> {
        // Audit trail: record destructive bot operations (and their outcome) at
        // INFO so there is a durable log entry beyond the transient TUI status
        // line. Tracing supplies the timestamp.
        if operation.is_destructive() {
            info!(bot_id = %bot_id, operation = operation.label(), "executing destructive bot operation");
        }

        let path = operation.path(bot_id);
        let result = api_request_json(client, &self.api_url, &path, Method::POST).await;
        if operation.is_destructive() {
            match &result {
                Ok(_) => info!(
                    bot_id = %bot_id,
                    operation = operation.label(),
                    "destructive bot operation submitted"
                ),
                Err(error) => info!(
                    bot_id = %bot_id,
                    operation = operation.label(),
                    error = %error,
                    "destructive bot operation failed"
                ),
            }
        }
        result?;

        let status_message = if operation.is_journaling_only() {
            format!(
                "{} requested for bot `{bot_id}` (journaling-only; no broker close/cancel yet)",
                operation.label()
            )
        } else {
            format!("{} requested for bot `{bot_id}`", operation.label())
        };
        self.set_status_message(status_message);
        self.refresh(client).await?;
        Ok(())
    }

    pub(super) async fn toggle_kill_switch(&mut self, client: &Client) -> Result<()> {
        let currently_active = self.snapshot.service.kill_switch_active;
        let path = if currently_active {
            "/v1/risk/clear-kill-switch"
        } else {
            "/v1/risk/kill-switch"
        };
        // Engaging the kill switch halts all trading and is destructive; audit it.
        if !currently_active {
            info!("executing destructive operation: engage kill switch");
        }
        let result = api_request_json(client, &self.api_url, path, Method::POST).await;
        if !currently_active {
            match &result {
                Ok(_) => info!("kill switch engaged"),
                Err(error) => info!(error = %error, "kill switch engage failed"),
            }
        }
        result?;
        let label = if currently_active {
            "kill switch cleared"
        } else {
            "kill switch enabled"
        };
        self.set_status_message(label);
        self.refresh(client).await?;
        Ok(())
    }
}

fn clamp_selected_index(current: usize, len: usize) -> usize {
    if len == 0 { 0 } else { current.min(len - 1) }
}

#[cfg(test)]
mod tests {
    use super::super::input::{request_bot_operation, resolve_pending_confirmation};
    use super::super::models::{DashboardBotPosition, DashboardWarmupStatus};
    use super::*;

    #[test]
    fn operation_paths_match_http_routes() {
        assert_eq!(BotOperation::Start.path("aapl"), "/v1/bots/aapl/start");
        assert_eq!(BotOperation::Stop.path("aapl"), "/v1/bots/aapl/stop");
        assert_eq!(BotOperation::Pause.path("aapl"), "/v1/bots/aapl/pause");
        assert_eq!(BotOperation::Resume.path("aapl"), "/v1/bots/aapl/resume");
        assert_eq!(BotOperation::Tick.path("aapl"), "/v1/bots/aapl/tick");
        assert_eq!(
            BotOperation::Reconcile.path("aapl"),
            "/v1/bots/aapl/reconcile"
        );
        assert_eq!(
            BotOperation::CancelOpenOrders.path("aapl"),
            "/v1/bots/aapl/cancel-open-orders"
        );
        assert_eq!(
            BotOperation::ClosePositions.path("aapl"),
            "/v1/bots/aapl/close-positions"
        );
    }

    fn bot_summary(id: &str) -> DashboardBotSummary {
        DashboardBotSummary {
            id: id.to_owned(),
            state: "running".to_owned(),
            market: "equities".to_owned(),
            timeframe: "1m".to_owned(),
            account: "alpaca-paper".to_owned(),
            execution_mode: "paper".to_owned(),
            position: DashboardBotPosition::default(),
            reconciliation_blocked: false,
            reconciliation_by_symbol: Vec::new(),
            warmup: DashboardWarmupStatus {
                required_bars: 0,
                loaded_bars: 0,
                ready: true,
                last_error: None,
            },
        }
    }

    fn app_with_bots(bots: Vec<DashboardBotSummary>) -> DashboardApp {
        let snapshot = DashboardSnapshot {
            bots,
            ..DashboardSnapshot::default()
        };
        DashboardApp {
            api_url: "http://127.0.0.1:0".to_owned(),
            limit: 50,
            refresh_interval: Duration::from_secs(1),
            selected_bot: 0,
            snapshot,
            status_message: String::new(),
            last_refresh_at: Instant::now(),
            pending_confirmation: None,
        }
    }

    #[tokio::test]
    async fn destructive_confirmation_captures_bot_id_not_index() {
        let client = Client::new();
        let mut app = app_with_bots(vec![bot_summary("alpha"), bot_summary("beta")]);
        app.selected_bot = 0;

        // Stage a destructive op against the selected bot (`alpha`).
        request_bot_operation(&mut app, &client, BotOperation::ClosePositions).await;
        let pending = app
            .pending_confirmation
            .clone()
            .expect("destructive op should stage a confirmation");
        match &pending {
            PendingConfirmation::BotOperation { bot_id, operation } => {
                assert_eq!(bot_id, "alpha");
                assert!(matches!(operation, BotOperation::ClosePositions));
            }
            other @ PendingConfirmation::EngageKillSwitch => {
                panic!("unexpected pending confirmation: {other:?}")
            }
        }

        // Simulate the refresh loop reordering bots so the selected index now
        // resolves to a *different* bot. The captured id must be unaffected.
        app.snapshot.bots = vec![bot_summary("beta"), bot_summary("alpha")];
        app.selected_bot = clamp_selected_index(app.selected_bot, app.snapshot.bots.len());
        assert_eq!(
            app.selected_bot_id(),
            Some("beta"),
            "index now resolves to a different bot"
        );
        match &pending {
            PendingConfirmation::BotOperation { bot_id, .. } => {
                assert_eq!(
                    bot_id, "alpha",
                    "confirmed bot id must remain the one the operator saw, not the re-clamped index"
                );
            }
            other @ PendingConfirmation::EngageKillSwitch => {
                panic!("unexpected pending confirmation: {other:?}")
            }
        }
    }

    #[tokio::test]
    async fn destructive_confirmation_aborts_when_confirmed_bot_is_gone() {
        let client = Client::new();
        let mut app = app_with_bots(vec![bot_summary("alpha"), bot_summary("beta")]);
        app.selected_bot = 0;

        request_bot_operation(&mut app, &client, BotOperation::CancelOpenOrders).await;
        let pending = app
            .pending_confirmation
            .take()
            .expect("destructive op should stage a confirmation");

        // The confirmed bot disappears (and the index would now point at `beta`).
        app.snapshot.bots = vec![bot_summary("beta")];
        app.selected_bot = clamp_selected_index(app.selected_bot, app.snapshot.bots.len());
        assert!(!app.has_bot("alpha"));
        assert_eq!(app.selected_bot_id(), Some("beta"));

        // Confirming must abort safely rather than acting on `beta`. The abort
        // branch returns before any network call, so this is server-free.
        resolve_pending_confirmation(&mut app, &client, pending).await;
        assert!(
            app.status_message.contains("alpha")
                && app.status_message.contains("no longer present"),
            "expected an abort-for-missing-bot message, got: {}",
            app.status_message
        );
    }

    #[test]
    fn clamp_selected_index_handles_empty_and_bounds() {
        assert_eq!(clamp_selected_index(0, 0), 0);
        assert_eq!(clamp_selected_index(2, 3), 2);
        assert_eq!(clamp_selected_index(5, 3), 2);
    }
}
