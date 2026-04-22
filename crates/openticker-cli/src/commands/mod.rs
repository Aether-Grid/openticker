use anyhow::Result;
use tracing::info;

use crate::cli::Commands;

mod config;
mod instance;
mod risk;
mod service;

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
