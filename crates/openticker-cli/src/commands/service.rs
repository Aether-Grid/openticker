use anyhow::Result;
use openticker_config::load_from_dir;
use openticker_http::{load_http_state, serve};
use std::path::PathBuf;
use tracing::info;

use crate::api::fetch_and_print;
use crate::cli::ServiceCommand;

pub(crate) async fn handle_service_command(command: ServiceCommand) -> Result<()> {
    match command {
        ServiceCommand::Run { config_dir, bind } => run_service(config_dir, bind).await,
        ServiceCommand::Status { api } => fetch_and_print(&api.api_url, "/v1/service/status").await,
        ServiceCommand::Ledger { api } => fetch_and_print(&api.api_url, "/v1/ledger").await,
        ServiceCommand::Connectors { api } => {
            fetch_and_print(&api.api_url, "/v1/connectors/status").await
        }
        ServiceCommand::ConnectorsMatrix { api } => {
            fetch_and_print(&api.api_url, "/v1/connectors/matrix").await
        }
        ServiceCommand::Events { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/events?limit={limit}")).await
        }
        ServiceCommand::Signals { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/signals?limit={limit}")).await
        }
        ServiceCommand::Intents { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/intents?limit={limit}")).await
        }
        ServiceCommand::RiskDecisions { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/risk-decisions?limit={limit}")).await
        }
        ServiceCommand::Orders { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/orders?limit={limit}")).await
        }
        ServiceCommand::Fills { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/fills?limit={limit}")).await
        }
        ServiceCommand::Positions { api, limit } => {
            fetch_and_print(&api.api_url, &format!("/v1/positions?limit={limit}")).await
        }
        ServiceCommand::Reconciliations {
            api,
            limit,
            instance_id,
        } => {
            let path = reconciliations_path(limit, instance_id.as_deref());
            fetch_and_print(&api.api_url, &path).await
        }
    }
}

async fn run_service(config_dir: PathBuf, bind: Option<String>) -> Result<()> {
    let bundle = load_from_dir(&config_dir)?;
    let bind = bind.unwrap_or_else(|| bundle.global.http.bind.clone());
    let state = load_http_state(&config_dir)?;

    info!(bind, config_dir = %config_dir.display(), "starting openticker-runtime service");
    println!("starting openticker-runtime on {bind}");
    serve(&bind, state).await
}

fn reconciliations_path(limit: usize, instance_id: Option<&str>) -> String {
    if let Some(instance_id) = instance_id {
        format!("/v1/reconciliations?limit={limit}&bot_id={instance_id}")
    } else {
        format!("/v1/reconciliations?limit={limit}")
    }
}
