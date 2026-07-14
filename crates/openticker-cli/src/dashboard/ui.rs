use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
};

use super::app::DashboardApp;
use super::models::{
    DashboardBotPosition, DashboardBotSummary, DashboardServiceStatus,
    DashboardSymbolReconciliationSummary,
};

const KEY_HINTS: &str = "keys: q quit | g refresh | up/down select | s start | x stop | p pause | r resume | t tick | y reconcile | o cancel-orders | l close-position | k kill-switch";

pub(super) fn render_dashboard(frame: &mut Frame, app: &DashboardApp) {
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
    // Only a couple of items ever survive the 72-char `trim_text` in the bots
    // list, so formatting every symbol when there are hundreds is wasted work.
    // Cap before formatting and surface the count of omitted symbols.
    const MAX_ITEMS: usize = 10;

    if items.is_empty() {
        return "n/a".to_owned();
    }

    let mut rendered = items
        .iter()
        .take(MAX_ITEMS)
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
        .join("; ");

    if let Some(remaining) = items.len().checked_sub(MAX_ITEMS).filter(|&n| n > 0) {
        use std::fmt::Write as _;
        let _ = write!(rendered, "; +{remaining} more");
    }

    rendered
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::models::DashboardWarmupStatus;

    #[test]
    fn trim_text_shortens_long_strings() {
        assert_eq!(trim_text("short", 10), "short");
        assert_eq!(trim_text("abcdefghi", 6), "abc...");
        assert_eq!(trim_text("abcdefghi", 3), "...");
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

    fn recon_item(symbol: &str) -> DashboardSymbolReconciliationSummary {
        DashboardSymbolReconciliationSummary {
            symbol: symbol.to_owned(),
            reconciliation_blocked: false,
            remote_net_qty: Some(1.0),
            aggregate_managed_qty: 1.0,
            external_delta_qty: Some(0.0),
            managed_remote_open_orders: 0,
            external_remote_open_orders: 0,
        }
    }

    #[test]
    fn format_reconciliation_caps_items_and_reports_overflow() {
        let items: Vec<DashboardSymbolReconciliationSummary> =
            (0..25).map(|i| recon_item(&format!("SYM{i}"))).collect();
        let rendered = format_reconciliation_by_symbol(&items);

        // Only the first 10 symbols are formatted; the rest are summarised.
        assert!(
            rendered.contains("SYM0("),
            "missing first symbol: {rendered}"
        );
        assert!(
            rendered.contains("SYM9("),
            "missing 10th symbol: {rendered}"
        );
        assert!(
            !rendered.contains("SYM10("),
            "11th symbol should not be formatted: {rendered}"
        );
        assert!(
            rendered.ends_with("; +15 more"),
            "missing overflow tag: {rendered}"
        );
    }

    #[test]
    fn format_reconciliation_without_overflow_has_no_more_tag() {
        let items = vec![recon_item("AAPL"), recon_item("MSFT")];
        let rendered = format_reconciliation_by_symbol(&items);
        assert!(
            !rendered.contains("more"),
            "unexpected overflow tag: {rendered}"
        );
        assert!(rendered.contains("AAPL("));
        assert!(rendered.contains("MSFT("));
    }

    #[test]
    fn format_reconciliation_empty_is_na() {
        assert_eq!(format_reconciliation_by_symbol(&[]), "n/a");
    }
}
