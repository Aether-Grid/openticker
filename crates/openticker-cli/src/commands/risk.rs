use anyhow::Result;

use crate::api::{fetch_and_print, post_and_print};
use crate::cli::{KillSwitchCommand, RiskCommand};

pub(crate) async fn handle_risk_command(command: RiskCommand) -> Result<()> {
    match command {
        RiskCommand::KillSwitch { command } => match command {
            KillSwitchCommand::On { api } => {
                post_and_print(&api.api_url, "/v1/risk/kill-switch").await
            }
            KillSwitchCommand::Off { api } => {
                post_and_print(&api.api_url, "/v1/risk/clear-kill-switch").await
            }
        },
        RiskCommand::Status { api } => fetch_and_print(&api.api_url, "/v1/service/status").await,
    }
}
