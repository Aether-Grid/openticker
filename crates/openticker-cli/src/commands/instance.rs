use anyhow::Result;
use serde_json::json;
use tokio::time::{Duration as TokioDuration, sleep};
use tracing::{debug, info};

use crate::api::{api_post_json, fetch_and_print, post_and_print, print_json};
use crate::cli::InstanceCommand;

pub(crate) async fn handle_instance_command(command: InstanceCommand) -> Result<()> {
    match command {
        InstanceCommand::List { api } => fetch_and_print(&api.api_url, "/v1/bots").await,
        InstanceCommand::Get { id, api } => {
            fetch_and_print(&api.api_url, &format!("/v1/bots/{id}")).await
        }
        InstanceCommand::Start { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/start")).await
        }
        InstanceCommand::Stop { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/stop")).await
        }
        InstanceCommand::Pause { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/pause")).await
        }
        InstanceCommand::Resume { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/resume")).await
        }
        InstanceCommand::Reconcile { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/reconcile")).await
        }
        InstanceCommand::ReconcileReport { id, api } => {
            fetch_and_print(
                &api.api_url,
                &format!("/v1/bots/{id}/reconciliation-report"),
            )
            .await
        }
        InstanceCommand::Tick { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/tick")).await
        }
        InstanceCommand::AutoTick {
            id,
            api,
            interval_ms,
            max_ticks,
        } => run_auto_tick(&api.api_url, &id, interval_ms, max_ticks).await,
        InstanceCommand::CancelOpenOrders { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/cancel-open-orders")).await
        }
        InstanceCommand::ClosePositions { id, api } => {
            post_and_print(&api.api_url, &format!("/v1/bots/{id}/close-positions")).await
        }
    }
}

async fn run_auto_tick(
    api_url: &str,
    id: &str,
    interval_ms: u64,
    max_ticks: Option<u64>,
) -> Result<()> {
    info!(
        instance_id = id,
        interval_ms,
        ?max_ticks,
        "starting auto-tick loop"
    );
    let mut ticks_sent = 0_u64;
    loop {
        let payload = api_post_json(api_url, &format!("/v1/bots/{id}/tick")).await?;
        let processed = payload.as_array().map_or(0, Vec::len);
        debug!(
            instance_id = id,
            tick = ticks_sent + 1,
            processed_outcomes = processed,
            "auto-tick cycle completed"
        );
        print_json(json!({
            "tick": ticks_sent + 1,
            "processed_outcomes": processed,
            "outcomes": payload
        }))?;

        ticks_sent = ticks_sent.saturating_add(1);
        if max_ticks.is_some_and(|max| ticks_sent >= max) {
            break;
        }

        sleep(TokioDuration::from_millis(interval_ms)).await;
    }

    info!(instance_id = id, ticks_sent, "auto-tick loop finished");
    Ok(())
}
