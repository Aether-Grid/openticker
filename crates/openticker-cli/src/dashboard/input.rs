use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use reqwest::Client;

use super::app::{BotOperation, DashboardApp, PendingConfirmation};

pub(super) async fn handle_key_event(
    app: &mut DashboardApp,
    client: &Client,
    key_event: KeyEvent,
) -> Result<bool> {
    if key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('c'))
    {
        return Ok(true);
    }

    // If a destructive action is awaiting confirmation, this keypress resolves
    // it: `y`/`Y` confirms, anything else cancels.
    if let Some(pending) = app.pending_confirmation.take() {
        match key_event.code {
            KeyCode::Char('y' | 'Y') => resolve_pending_confirmation(app, client, pending).await,
            _ => app.set_status_message("cancelled"),
        }
        return Ok(false);
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
            // Only engaging the kill switch is destructive; clearing it is a
            // recovery action and runs immediately.
            if app.snapshot.service.kill_switch_active {
                if let Err(error) = app.toggle_kill_switch(client).await {
                    app.set_status_message(format!("kill switch request failed: {error}"));
                }
            } else {
                app.pending_confirmation = Some(PendingConfirmation::EngageKillSwitch);
                app.set_status_message(
                    "confirm: engage kill switch (halts all trading)? press y to confirm, any other key to cancel",
                );
            }
        }
        KeyCode::Char('s') => {
            request_bot_operation(app, client, BotOperation::Start).await;
        }
        KeyCode::Char('x') => {
            request_bot_operation(app, client, BotOperation::Stop).await;
        }
        KeyCode::Char('p') => {
            request_bot_operation(app, client, BotOperation::Pause).await;
        }
        KeyCode::Char('r') => {
            request_bot_operation(app, client, BotOperation::Resume).await;
        }
        KeyCode::Char('t') => {
            request_bot_operation(app, client, BotOperation::Tick).await;
        }
        KeyCode::Char('y') => {
            request_bot_operation(app, client, BotOperation::Reconcile).await;
        }
        KeyCode::Char('o') => {
            request_bot_operation(app, client, BotOperation::CancelOpenOrders).await;
        }
        KeyCode::Char('l') => {
            request_bot_operation(app, client, BotOperation::ClosePositions).await;
        }
        _ => {}
    }

    Ok(false)
}

async fn run_bot_operation(
    app: &mut DashboardApp,
    client: &Client,
    operation: BotOperation,
    bot_id: &str,
) {
    if let Err(error) = app.execute_bot_operation(client, operation, bot_id).await {
        app.set_status_message(format!("{} failed: {error}", operation.label()));
    }
}

/// Routes a bot operation: destructive ones are staged for a confirmation
/// keypress, non-destructive ones run immediately.
pub(super) async fn request_bot_operation(
    app: &mut DashboardApp,
    client: &Client,
    operation: BotOperation,
) {
    let Some(bot_id) = app.selected_bot_id() else {
        app.set_status_message("no bot selected");
        return;
    };
    let bot_id = bot_id.to_owned();

    if !operation.is_destructive() {
        run_bot_operation(app, client, operation, &bot_id).await;
        return;
    }

    app.set_status_message(format!(
        "confirm: {} for bot `{bot_id}`? press y to confirm, any other key to cancel",
        operation.label()
    ));
    // Capture the resolved bot id alongside the operation so the confirmed
    // action targets exactly the bot named in the prompt, even if the snapshot
    // reorders or drops bots before the operator presses `y`.
    app.pending_confirmation = Some(PendingConfirmation::BotOperation { operation, bot_id });
}

/// Executes a previously-staged destructive action after confirmation.
pub(super) async fn resolve_pending_confirmation(
    app: &mut DashboardApp,
    client: &Client,
    pending: PendingConfirmation,
) {
    match pending {
        PendingConfirmation::BotOperation { operation, bot_id } => {
            // The operator confirmed a *specific* bot. If the snapshot changed
            // between the two keypresses and that bot is no longer present, do
            // nothing destructive rather than silently acting on whichever bot
            // the selected index now points at.
            if !app.has_bot(&bot_id) {
                app.set_status_message(format!(
                    "{} cancelled: bot `{bot_id}` is no longer present",
                    operation.label()
                ));
                return;
            }
            run_bot_operation(app, client, operation, &bot_id).await;
        }
        PendingConfirmation::EngageKillSwitch => {
            if let Err(error) = app.toggle_kill_switch(client).await {
                app.set_status_message(format!("kill switch request failed: {error}"));
            }
        }
    }
}
