use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Result, bail};
use tracing::info;

use crate::cli::Commands;

mod config;
mod instance;
mod risk;
mod service;

/// Confirms a destructive operation before it executes.
///
/// - If `--yes` was passed, proceed immediately (logging the bypass).
/// - Otherwise, if stdin is an interactive TTY, prompt `y/N` and proceed only on
///   an explicit yes.
/// - Otherwise (non-interactive, e.g. a script or pipe), refuse with an error so
///   an accidental invocation can never silently liquidate positions or trip the
///   kill switch.
///
/// Returns `Ok(true)` to proceed and `Ok(false)` when an interactive user
/// declines.
///
/// # Errors
///
/// Returns an error when confirmation is required but cannot be obtained
/// non-interactively, or when prompting I/O fails.
pub(crate) fn confirm_destructive(action: &str, yes: bool) -> Result<bool> {
    if yes {
        info!(action, "destructive operation confirmed via --yes");
        return Ok(true);
    }

    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        bail!(
            "refusing to run destructive operation `{action}` without confirmation; \
             re-run with --yes (no interactive terminal available to prompt)"
        );
    }

    let mut stdout = std::io::stdout();
    write!(
        stdout,
        "About to {action}. This is destructive. Continue? [y/N] "
    )
    .and_then(|()| stdout.flush())?;

    let mut answer = String::new();
    stdin.lock().read_line(&mut answer)?;
    let confirmed = matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !confirmed {
        info!(action, "destructive operation declined by operator");
    }
    Ok(confirmed)
}

pub(crate) async fn dispatch_command(command: Commands) -> Result<()> {
    info!(?command, "executing command");
    match command {
        Commands::Dashboard { options } => crate::dashboard::run(options).await,
        Commands::ValidateConfig { config_dir } => config::validate_config(config_dir.as_path()),
        Commands::Config { command } => config::handle_config_command(command).await,
        Commands::Service { command } => service::handle_service_command(command).await,
        Commands::Risk { command } => risk::handle_risk_command(command).await,
        Commands::Instance { command } => instance::handle_instance_command(command).await,
    }
}
