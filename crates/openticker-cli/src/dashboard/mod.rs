use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use reqwest::Client;
use std::time::Duration;

use crate::cli::DashboardOptions;

mod app;
mod client;
mod input;
mod models;
mod terminal;
mod ui;

use app::DashboardApp;
use input::handle_key_event;
use terminal::{TerminalGuard, create_terminal};
use ui::render_dashboard;

pub(crate) async fn run(options: DashboardOptions) -> Result<()> {
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
