use anyhow::Result;
use openticker_config::load_from_dir;
use std::path::Path;

use crate::api::{post_and_print, print_json};
use crate::cli::ConfigCommand;

pub(crate) fn validate_config(config_dir: &Path) -> Result<()> {
    let bundle = load_from_dir(config_dir)?;
    println!(
        "config is valid: {} account(s), {} risk profile(s), {} instance(s)",
        bundle.accounts.len(),
        bundle.risk_profiles.len(),
        bundle.instances.len()
    );
    Ok(())
}

pub(crate) async fn handle_config_command(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Print {
            effective,
            config_dir,
        } => {
            let bundle = load_from_dir(&config_dir)?;
            if effective {
                print_json(bundle.effective_config())?;
            } else {
                print_json(bundle)?;
            }
            Ok(())
        }
        ConfigCommand::Reload { api } => post_and_print(&api.api_url, "/v1/config/reload").await,
    }
}
