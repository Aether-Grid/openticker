use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
};
use reqwest::{Client, Method};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crate::DashboardOptions;

const KEY_HINTS: &str = "keys: q quit | g refresh | up/down select | s start | x stop | p pause | r resume | t tick | y reconcile | o cancel-orders | l close-position | k kill-switch";

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardServiceStatus {
    #[serde(default)]
    total_instances: usize,
    #[serde(default)]
    running_instances: usize,
    #[serde(default)]
    paused_instances: usize,
    #[serde(default)]
    stopped_instances: usize,
    #[serde(default)]
    reconciling_instances: usize,
    #[serde(default)]
    reconciliation_blocked_instances: usize,
    #[serde(default)]
    warmup_ready_instances: usize,
    #[serde(default)]
    warmup_pending_instances: usize,
    #[serde(default)]
    warmup_failed_instances: usize,
    #[serde(default)]
    kill_switch_active: bool,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    live_mode_active: bool,
    #[serde(default)]
    mode_banner: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardWarmupStatus {
    #[serde(default)]
    required_bars: usize,
    #[serde(default)]
    loaded_bars: usize,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardBotPosition {
    #[serde(default)]
    has_position: bool,
    #[serde(default)]
    quantity: f64,
    #[serde(default)]
    entry_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardBotSummary {
    #[serde(default)]
    id: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    market: String,
    #[serde(default)]
    timeframe: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    execution_mode: String,
    #[serde(default)]
    position: DashboardBotPosition,
    #[serde(default)]
    reconciliation_blocked: bool,
    #[serde(default)]
    reconciliation_by_symbol: Vec<DashboardSymbolReconciliationSummary>,
    #[serde(default)]
    warmup: DashboardWarmupStatus,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardSymbolReconciliationSummary {
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    reconciliation_blocked: bool,
    #[serde(default)]
    remote_net_qty: Option<f64>,
    #[serde(default)]
    aggregate_managed_qty: f64,
    #[serde(default)]
    external_delta_qty: Option<f64>,
    #[serde(default)]
    managed_remote_open_orders: usize,
    #[serde(default)]
    external_remote_open_orders: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardConnectorStatus {
    #[serde(default)]
    account_id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardRiskDecisionRecord {
    #[serde(default, rename = "bot_id", alias = "instance_id")]
    bot_id: String,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    intent: String,
    #[serde(default)]
    decision: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardRiskDecisionResponse {
    #[serde(default)]
    count: usize,
    #[serde(default)]
    items: Vec<DashboardRiskDecisionRecord>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardOrderRecord {
    #[serde(default, rename = "bot_id", alias = "instance_id")]
    bot_id: String,
    #[serde(default)]
    intent: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    quantity: f64,
    #[serde(default)]
    created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardRuntimeEvent {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    payload: String,
    #[serde(default)]
    created_at_ms: i64,
}

#[derive(Debug, Default)]
struct DashboardSnapshot {
    service: DashboardServiceStatus,
    bots: Vec<DashboardBotSummary>,
    connectors: Vec<DashboardConnectorStatus>,
    risk_count: usize,
    risk_decisions: Vec<DashboardRiskDecisionRecord>,
    orders: Vec<DashboardOrderRecord>,
    events: Vec<DashboardRuntimeEvent>,
}

#[derive(Debug, Clone, Copy)]
enum BotOperation {
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
    fn label(self) -> &'static str {
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

    fn path(self, bot_id: &str) -> String {
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

#[derive(Debug)]
struct DashboardApp {
    api_url: String,
    limit: usize,
    refresh_interval: Duration,
    selected_bot: usize,
    snapshot: DashboardSnapshot,
    status_message: String,
    last_refresh_at: Instant,
}

impl DashboardApp {
    fn new(options: DashboardOptions) -> Self {
        Self {
            api_url: options.api.api_url,
            limit: options.limit.clamp(1, 200),
            refresh_interval: Duration::from_millis(options.refresh_ms.max(250)),
            selected_bot: 0,
            snapshot: DashboardSnapshot::default(),
            status_message: "starting dashboard".to_owned(),
            last_refresh_at: Instant::now(),
        }
    }

    fn selected_bot(&self) -> Option<&DashboardBotSummary> {
        self.snapshot.bots.get(self.selected_bot)
    }

    fn selected_bot_id(&self) -> Option<&str> {
        self.selected_bot().map(|bot| bot.id.as_str())
    }

    fn select_next(&mut self) {
        if self.snapshot.bots.is_empty() {
            return;
        }
        self.selected_bot = (self.selected_bot + 1) % self.snapshot.bots.len();
    }

    fn select_previous(&mut self) {
        if self.snapshot.bots.is_empty() {
            return;
        }
        self.selected_bot = if self.selected_bot == 0 {
            self.snapshot.bots.len() - 1
        } else {
            self.selected_bot - 1
        };
    }

    fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
    }

    fn should_auto_refresh(&self) -> bool {
        self.last_refresh_at.elapsed() >= self.refresh_interval
    }

    async fn refresh(&mut self, client: &Client) -> Result<()> {
        let snapshot = fetch_snapshot(client, &self.api_url, self.limit).await?;
        self.snapshot = snapshot;
        self.selected_bot = clamp_selected_index(self.selected_bot, self.snapshot.bots.len());
        self.last_refresh_at = Instant::now();
        Ok(())
    }

    async fn execute_bot_operation(
        &mut self,
        client: &Client,
        operation: BotOperation,
    ) -> Result<()> {
        let bot_id = if let Some(bot_id) = self.selected_bot_id() {
            bot_id.to_owned()
        } else {
            self.set_status_message("no bot selected");
            return Ok(());
        };

        let path = operation.path(&bot_id);
        let _response = api_request_json(client, &self.api_url, &path, Method::POST).await?;
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

    async fn toggle_kill_switch(&mut self, client: &Client) -> Result<()> {
        let currently_active = self.snapshot.service.kill_switch_active;
        let path = if currently_active {
            "/v1/risk/clear-kill-switch"
        } else {
            "/v1/risk/kill-switch"
        };
        let _response = api_request_json(client, &self.api_url, path, Method::POST).await?;
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

pub(super) async fn run(options: DashboardOptions) -> Result<()> {
    let client = Client::new();
    let mut app = DashboardApp::new(options);
    app.refresh(&client)
        .await
        .with_context(|| format!("failed to refresh dashboard from {}", app.api_url))?;
    app.set_status_message("dashboard ready");

    let _terminal_guard = TerminalGuard::enter()?;
    let mut terminal = create_terminal()?;
    terminal.clear().context("failed to clear terminal")?;

    loop {
        terminal
            .draw(|frame| render_dashboard(frame, &app))
            .context("failed to draw dashboard")?;

        if event::poll(Duration::from_millis(100)).context("failed to poll terminal event")? {
            let next_event = event::read().context("failed to read terminal event")?;
            if let Event::Key(key_event) = next_event {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_key_event(&mut app, &client, key_event).await? {
                    break;
                }
            }
        }

        if app.should_auto_refresh()
            && let Err(error) = app.refresh(&client).await
        {
            app.set_status_message(format!("refresh failed: {error}"));
        }
    }

    terminal.show_cursor().context("failed to restore cursor")?;
    Ok(())
}

async fn handle_key_event(
    app: &mut DashboardApp,
    client: &Client,
    key_event: KeyEvent,
) -> Result<bool> {
    if key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('c'))
    {
        return Ok(true);
    }

    match key_event.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Up => app.select_previous(),
        KeyCode::Down => app.select_next(),
        KeyCode::Char('g') => {
            if let Err(error) = app.refresh(client).await {
                app.set_status_message(format!("refresh failed: {error}"));
            }
        }
        KeyCode::Char('k') => {
            if let Err(error) = app.toggle_kill_switch(client).await {
                app.set_status_message(format!("kill switch request failed: {error}"));
            }
        }
        KeyCode::Char('s') => {
            run_bot_operation(app, client, BotOperation::Start).await;
        }
        KeyCode::Char('x') => {
            run_bot_operation(app, client, BotOperation::Stop).await;
        }
        KeyCode::Char('p') => {
            run_bot_operation(app, client, BotOperation::Pause).await;
        }
        KeyCode::Char('r') => {
            run_bot_operation(app, client, BotOperation::Resume).await;
        }
        KeyCode::Char('t') => {
            run_bot_operation(app, client, BotOperation::Tick).await;
        }
        KeyCode::Char('y') => {
            run_bot_operation(app, client, BotOperation::Reconcile).await;
        }
        KeyCode::Char('o') => {
            run_bot_operation(app, client, BotOperation::CancelOpenOrders).await;
        }
        KeyCode::Char('l') => {
            run_bot_operation(app, client, BotOperation::ClosePositions).await;
        }
        _ => {}
    }

    Ok(false)
}

async fn run_bot_operation(app: &mut DashboardApp, client: &Client, operation: BotOperation) {
    if let Err(error) = app.execute_bot_operation(client, operation).await {
        app.set_status_message(format!("{} failed: {error}", operation.label()));
    }
}

fn render_dashboard(frame: &mut Frame, app: &DashboardApp) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(10),
            Constraint::Length(9),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, sections[0], app);
    render_service_summary(frame, sections[1], app);

    let main_sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(sections[2]);
    render_bots(frame, main_sections[0], app);
    render_connectors(frame, main_sections[1], app);

    let lower_sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(sections[3]);
    render_risk_decisions(frame, lower_sections[0], app);
    render_orders(frame, lower_sections[1], app);
    render_events(frame, lower_sections[2], app);

    render_status(frame, sections[4], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let mode_banner = if app.snapshot.service.mode_banner.is_empty() {
        if app.snapshot.service.live_mode_active {
            "LIVE MODE ACTIVE - real capital may be at risk"
        } else {
            "PAPER MODE - non-live execution path"
        }
    } else {
        app.snapshot.service.mode_banner.as_str()
    };
    let mode_style = if app.snapshot.service.live_mode_active {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "OpenTicker Dashboard",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  api={}  refresh={}ms  limit={}",
            app.api_url,
            app.refresh_interval.as_millis(),
            app.limit
        )),
        Span::styled(format!("  {mode_banner}"), mode_style),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, area);
}

fn selected_bot_status_line(selected: Option<&DashboardBotSummary>) -> String {
    selected.map_or_else(
        || "selected=none".to_owned(),
        |bot| {
            format!(
                "selected={} state={} mode={} market={} tf={} blocked={} warmup={}/{} ready={} pos={} recon={}",
                bot.id,
                bot.state,
                bot.execution_mode,
                bot.market,
                bot.timeframe,
                bot.reconciliation_blocked,
                bot.warmup.loaded_bars,
                bot.warmup.required_bars,
                bot.warmup.ready,
                format_bot_position(&bot.position),
                format_reconciliation_by_symbol(&bot.reconciliation_by_symbol),
            )
        },
    )
}

fn service_status_line(service: &DashboardServiceStatus) -> String {
    format!(
        "ready={} kill_switch={} total={} running={} paused={} stopped={} reconciling={} blocked={} warmup_ready={} warmup_pending={} warmup_failed={}",
        service.ready,
        service.kill_switch_active,
        service.total_instances,
        service.running_instances,
        service.paused_instances,
        service.stopped_instances,
        service.reconciling_instances,
        service.reconciliation_blocked_instances,
        service.warmup_ready_instances,
        service.warmup_pending_instances,
        service.warmup_failed_instances,
    )
}

fn render_service_summary(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let service = &app.snapshot.service;
    let selected = selected_bot_status_line(app.selected_bot());
    let summary = Paragraph::new(vec![
        Line::from(service_status_line(service)),
        Line::from(selected),
        Line::from(KEY_HINTS),
    ])
    .wrap(Wrap { trim: true })
    .block(Block::default().title("Service").borders(Borders::ALL));
    frame.render_widget(summary, area);
}

fn render_bots(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let items: Vec<ListItem> = if app.snapshot.bots.is_empty() {
        vec![ListItem::new("no bots")]
    } else {
        app.snapshot
            .bots
            .iter()
            .map(|bot| {
                ListItem::new(format!(
                    "{} | {} | {} {} | {} | acct={} | warmup={}/{}{} | pos={} | recon={}",
                    bot.id,
                    bot.state,
                    bot.market,
                    bot.timeframe,
                    bot.execution_mode,
                    bot.account,
                    bot.warmup.loaded_bars,
                    bot.warmup.required_bars,
                    bot.warmup
                        .last_error
                        .as_ref()
                        .map_or(String::new(), |_| " err".to_owned()),
                    format_bot_position(&bot.position),
                    trim_text(
                        &format_reconciliation_by_symbol(&bot.reconciliation_by_symbol),
                        72
                    ),
                ))
            })
            .collect()
    };

    let list = List::new(items)
        .block(Block::default().title("Bots").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let mut list_state = ListState::default();
    if !app.snapshot.bots.is_empty() {
        list_state.select(Some(app.selected_bot));
    }
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn format_optional_quantity(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.4}"))
}

fn format_reconciliation_by_symbol(items: &[DashboardSymbolReconciliationSummary]) -> String {
    if items.is_empty() {
        return "n/a".to_owned();
    }

    items
        .iter()
        .map(|item| {
            format!(
                "{}(blocked={},remote={},managed={:.4},delta={},orders={}/{})",
                item.symbol,
                item.reconciliation_blocked,
                format_optional_quantity(item.remote_net_qty),
                item.aggregate_managed_qty,
                format_optional_quantity(item.external_delta_qty),
                item.managed_remote_open_orders,
                item.external_remote_open_orders,
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn render_connectors(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let header = Row::new(["account", "kind", "mode", "state", "message"]).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let mut rows: Vec<Row> = app
        .snapshot
        .connectors
        .iter()
        .take(8)
        .map(|connector| {
            Row::new(vec![
                Cell::from(connector.account_id.clone()),
                Cell::from(connector.kind.clone()),
                Cell::from(connector.mode.clone()),
                Cell::from(connector.state.clone()),
                Cell::from(trim_text(&connector.message, 48)),
            ])
        })
        .collect();

    if rows.is_empty() {
        rows.push(Row::new(vec![
            Cell::from("-"),
            Cell::from("-"),
            Cell::from("-"),
            Cell::from("-"),
            Cell::from("no connector status"),
        ]));
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().title("Connectors").borders(Borders::ALL));
    frame.render_widget(table, area);
}

fn render_risk_decisions(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let title = format!("Risk Decisions ({})", app.snapshot.risk_count);
    let items: Vec<ListItem> = if app.snapshot.risk_decisions.is_empty() {
        vec![ListItem::new("no risk decisions")]
    } else {
        app.snapshot
            .risk_decisions
            .iter()
            .take(6)
            .map(|decision| {
                let reason = decision.reason.as_deref().unwrap_or("none");
                ListItem::new(format!(
                    "{} {} {} {} reason={} ts={}",
                    decision.bot_id,
                    decision.symbol.as_deref().unwrap_or("-"),
                    decision.decision,
                    decision.intent,
                    trim_text(reason, 22),
                    decision.created_at_ms
                ))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_orders(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let items: Vec<ListItem> = if app.snapshot.orders.is_empty() {
        vec![ListItem::new("no orders")]
    } else {
        app.snapshot
            .orders
            .iter()
            .take(6)
            .map(|order| {
                ListItem::new(format!(
                    "{} {} {} px={:.2} qty={:.2} ts={}",
                    order.bot_id,
                    order.intent,
                    order.status,
                    order.price,
                    order.quantity,
                    order.created_at_ms
                ))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default().title("Orders").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_events(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let items: Vec<ListItem> = if app.snapshot.events.is_empty() {
        vec![ListItem::new("no events")]
    } else {
        app.snapshot
            .events
            .iter()
            .take(6)
            .map(|event| {
                ListItem::new(format!(
                    "{} {} {} payload={} ts={}",
                    event.scope,
                    event.entity_id.as_deref().unwrap_or("-"),
                    event.kind,
                    trim_text(&event.payload, 20),
                    event.created_at_ms
                ))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default().title("Events").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let status = Paragraph::new(app.status_message.as_str())
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Status").borders(Borders::ALL));
    frame.render_widget(status, area);
}

async fn fetch_snapshot(client: &Client, api_url: &str, limit: usize) -> Result<DashboardSnapshot> {
    let service =
        api_get_typed::<DashboardServiceStatus>(client, api_url, "/v1/service/status").await?;
    let bots = api_get_typed::<Vec<DashboardBotSummary>>(client, api_url, "/v1/bots").await?;
    let connectors =
        api_get_typed::<Vec<DashboardConnectorStatus>>(client, api_url, "/v1/connectors/status")
            .await?;
    let risk_path = format!("/v1/risk-decisions?limit={limit}");
    let risk_response =
        api_get_typed::<DashboardRiskDecisionResponse>(client, api_url, &risk_path).await?;
    let orders_path = format!("/v1/orders?limit={limit}");
    let orders = api_get_typed::<Vec<DashboardOrderRecord>>(client, api_url, &orders_path).await?;
    let events_path = format!("/v1/events?limit={limit}");
    let events = api_get_typed::<Vec<DashboardRuntimeEvent>>(client, api_url, &events_path).await?;

    Ok(DashboardSnapshot {
        service,
        bots,
        connectors,
        risk_count: risk_response.count,
        risk_decisions: risk_response.items,
        orders,
        events,
    })
}

async fn api_get_typed<T: DeserializeOwned>(
    client: &Client,
    api_url: &str,
    path: &str,
) -> Result<T> {
    let payload = api_request_json(client, api_url, path, Method::GET).await?;
    serde_json::from_value(payload)
        .with_context(|| format!("response from `{path}` was not valid JSON shape"))
}

async fn api_request_json(
    client: &Client,
    api_url: &str,
    path: &str,
    method: Method,
) -> Result<Value> {
    let url = format!("{}{}", api_url.trim_end_matches('/'), path);
    let response = client
        .request(method, &url)
        .send()
        .await
        .with_context(|| format!("failed to send request to {url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("failed to read response body from {url}"))?;

    if !status.is_success() {
        bail!("request to {url} failed ({status}): {body}");
    }

    serde_json::from_str::<Value>(&body)
        .with_context(|| format!("response from {url} was not valid JSON"))
}

fn create_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::new(backend).context("failed to initialize ratatui terminal")
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        execute!(io::stdout(), EnterAlternateScreen).context("failed to enter alternate screen")?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn format_bot_position(position: &DashboardBotPosition) -> String {
    let open = position.has_position || position.quantity > f64::EPSILON;
    if !open {
        return "flat".to_owned();
    }

    if let Some(entry) = position.entry_price {
        format!("open qty={:.4} entry={entry:.4}", position.quantity)
    } else {
        format!("open qty={:.4}", position.quantity)
    }
}

fn trim_text(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_owned();
    }
    if max_len <= 3 {
        return "...".to_owned();
    }

    let mut truncated = value.chars().take(max_len - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn clamp_selected_index(current: usize, len: usize) -> usize {
    if len == 0 { 0 } else { current.min(len - 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn trim_text_shortens_long_strings() {
        assert_eq!(trim_text("short", 10), "short");
        assert_eq!(trim_text("abcdefghi", 6), "abc...");
        assert_eq!(trim_text("abcdefghi", 3), "...");
    }

    #[test]
    fn clamp_selected_index_handles_empty_and_bounds() {
        assert_eq!(clamp_selected_index(0, 0), 0);
        assert_eq!(clamp_selected_index(2, 3), 2);
        assert_eq!(clamp_selected_index(5, 3), 2);
    }

    #[test]
    fn dashboard_models_deserialize_warmup_fields() {
        let service: DashboardServiceStatus = serde_json::from_value(json!({
            "total_instances": 2,
            "warmup_ready_instances": 1,
            "warmup_pending_instances": 1,
            "warmup_failed_instances": 0,
            "ready": false
        }))
        .unwrap();
        assert_eq!(service.total_instances, 2);
        assert_eq!(service.warmup_ready_instances, 1);
        assert_eq!(service.warmup_pending_instances, 1);
        assert_eq!(service.warmup_failed_instances, 0);

        let bot: DashboardBotSummary = serde_json::from_value(json!({
            "id": "aapl",
            "state": "paused",
            "market": "equities",
            "timeframe": "1m",
            "account": "alpaca-paper",
            "execution_mode": "paper",
            "position": {
                "has_position": true,
                "quantity": 1.5,
                "entry_price": 123.45
            },
            "reconciliation_blocked": false,
            "reconciliation_by_symbol": [
                {
                    "symbol": "AAPL",
                    "reconciliation_blocked": false,
                    "remote_net_qty": 1.0,
                    "aggregate_managed_qty": 1.5,
                    "external_delta_qty": -0.5,
                    "managed_remote_open_orders": 1,
                    "external_remote_open_orders": 0
                }
            ],
            "warmup": {
                "required_bars": 200,
                "loaded_bars": 35,
                "ready": false,
                "last_error": "startup warmup backfill unavailable"
            }
        }))
        .unwrap();
        assert_eq!(bot.warmup.required_bars, 200);
        assert_eq!(bot.warmup.loaded_bars, 35);
        assert!(!bot.warmup.ready);
        assert_eq!(
            bot.warmup.last_error.as_deref(),
            Some("startup warmup backfill unavailable")
        );
        assert!(bot.position.has_position);
        assert!((bot.position.quantity - 1.5).abs() < f64::EPSILON);
        assert_eq!(bot.position.entry_price, Some(123.45));
        assert_eq!(bot.reconciliation_by_symbol.len(), 1);
        assert_eq!(bot.reconciliation_by_symbol[0].symbol, "AAPL");
        assert_eq!(bot.reconciliation_by_symbol[0].remote_net_qty, Some(1.0));
    }

    #[test]
    fn dashboard_status_shows_reconciliation_blocked_instances() {
        let service = DashboardServiceStatus {
            total_instances: 2,
            running_instances: 0,
            paused_instances: 2,
            stopped_instances: 0,
            reconciling_instances: 0,
            reconciliation_blocked_instances: 1,
            warmup_ready_instances: 2,
            warmup_pending_instances: 0,
            warmup_failed_instances: 0,
            kill_switch_active: false,
            ready: false,
            live_mode_active: false,
            mode_banner: "paper mode".to_owned(),
        };
        let blocked_bot = DashboardBotSummary {
            id: "aapl".to_owned(),
            state: "paused".to_owned(),
            market: "equities".to_owned(),
            timeframe: "1m".to_owned(),
            account: "alpaca-paper".to_owned(),
            execution_mode: "paper".to_owned(),
            position: DashboardBotPosition::default(),
            reconciliation_blocked: true,
            reconciliation_by_symbol: vec![DashboardSymbolReconciliationSummary {
                symbol: "AAPL".to_owned(),
                reconciliation_blocked: true,
                remote_net_qty: Some(1.0),
                aggregate_managed_qty: 1.5,
                external_delta_qty: Some(-0.5),
                managed_remote_open_orders: 1,
                external_remote_open_orders: 0,
            }],
            warmup: DashboardWarmupStatus {
                required_bars: 200,
                loaded_bars: 200,
                ready: true,
                last_error: None,
            },
        };

        let service_line = service_status_line(&service);
        let selected_line = selected_bot_status_line(Some(&blocked_bot));

        assert!(service_line.contains("ready=false"));
        assert!(service_line.contains("paused=2"));
        assert!(service_line.contains("blocked=1"));

        assert!(selected_line.contains("selected=aapl"));
        assert!(selected_line.contains("state=paused"));
        assert!(selected_line.contains("blocked=true"));
        assert!(
            selected_line.contains(
                "AAPL(blocked=true,remote=1.0000,managed=1.5000,delta=-0.5000,orders=1/0)"
            )
        );
    }
}
