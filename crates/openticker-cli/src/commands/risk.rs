use anyhow::Result;
use tracing::info;

use crate::api::{fetch_and_print, post_and_print};
use crate::cli::{KillSwitchCommand, RiskCommand};
use crate::commands::confirm_destructive;

pub(crate) async fn handle_risk_command(command: RiskCommand) -> Result<()> {
    match command {
        RiskCommand::KillSwitch { command } => match command {
            KillSwitchCommand::On { api, yes } => {
                if !confirm_destructive("engage the risk kill switch (halts all trading)", yes)? {
                    println!("aborted: kill-switch not confirmed");
                    return Ok(());
                }
                info!("kill-switch engage confirmed; submitting request");
                post_and_print(&api.api_url, "/v1/risk/kill-switch").await
            }
            KillSwitchCommand::Off { api } => {
                post_and_print(&api.api_url, "/v1/risk/clear-kill-switch").await
            }
        },
        RiskCommand::Status { api } => fetch_and_print(&api.api_url, "/v1/service/status").await,
    }
}
