use anyhow::Result;
use clap::Parser;
use tracing::{error, info};

mod api;
mod cli;
mod commands;
mod dashboard;
mod tracing_setup;

use crate::cli::Cli;
use crate::commands::dispatch_command;
use crate::tracing_setup::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();
    info!("openticker CLI starting");

    if let Err(error) = run().await {
        error!(error = %error, "command failed");
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch_command(cli.command).await
}
