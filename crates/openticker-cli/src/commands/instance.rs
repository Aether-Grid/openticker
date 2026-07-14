use anyhow::Result;
use serde_json::json;
use tokio::time::{Duration as TokioDuration, sleep};
use tracing::{debug, info, warn};

use crate::api::{
    api_post_json_with_client, build_client, encode_path_segment, fetch_and_print, post_and_print,
    print_json,
};
use crate::cli::{InstanceCommand, MIN_AUTO_TICK_INTERVAL_MS};
use crate::commands::confirm_destructive;

pub(crate) async fn handle_instance_command(command: InstanceCommand) -> Result<()> {
    match command {
        InstanceCommand::List { api } => fetch_and_print(&api.api_url, "/v1/bots").await,
        InstanceCommand::Get { id, api } => {
            fetch_and_print(
                &api.api_url,
                &format!("/v1/bots/{}", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::Start { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/start", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::Stop { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/stop", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::Pause { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/pause", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::Resume { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/resume", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::Reconcile { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/reconcile", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::ReconcileReport { id, api } => {
            fetch_and_print(
                &api.api_url,
                &format!(
                    "/v1/bots/{}/reconciliation-report",
                    encode_path_segment(&id)
                ),
            )
            .await
        }
        InstanceCommand::Tick { id, api } => {
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/tick", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::AutoTick {
            id,
            api,
            interval_ms,
            max_ticks,
        } => run_auto_tick(&api.api_url, &id, interval_ms, max_ticks).await,
        InstanceCommand::CancelOpenOrders { id, api, yes } => {
            if !confirm_destructive(&format!("cancel open orders for bot `{id}`"), yes)? {
                println!("aborted: cancel-open-orders not confirmed");
                return Ok(());
            }
            info!(instance_id = %id, "cancel-open-orders confirmed; submitting request");
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/cancel-open-orders", encode_path_segment(&id)),
            )
            .await
        }
        InstanceCommand::ClosePositions { id, api, yes } => {
            if !confirm_destructive(&format!("close positions for bot `{id}`"), yes)? {
                println!("aborted: close-positions not confirmed");
                return Ok(());
            }
            info!(instance_id = %id, "close-positions confirmed; submitting request");
            post_and_print(
                &api.api_url,
                &format!("/v1/bots/{}/close-positions", encode_path_segment(&id)),
            )
            .await
        }
    }
}

async fn run_auto_tick(
    api_url: &str,
    id: &str,
    interval_ms: u64,
    max_ticks: Option<u64>,
) -> Result<()> {
    // The clap value parser already clamps `interval_ms`, but re-clamp defensively
    // so an `interval_ms = 0` can never busy-loop this command even if the value
    // arrives through another path. Warn when we have to raise it so the operator
    // sees that their requested cadence was overridden.
    let interval_ms = if interval_ms < MIN_AUTO_TICK_INTERVAL_MS {
        warn!(
            requested_interval_ms = interval_ms,
            clamped_interval_ms = MIN_AUTO_TICK_INTERVAL_MS,
            "auto-tick interval below minimum; clamping up to avoid a busy loop"
        );
        MIN_AUTO_TICK_INTERVAL_MS
    } else {
        interval_ms
    };

    info!(
        instance_id = id,
        interval_ms,
        ?max_ticks,
        "starting auto-tick loop"
    );
    // Build the HTTP client once and reuse it across ticks so the loop shares a
    // single connection pool instead of rebuilding one (and reopening
    // connections) every iteration.
    let client = build_client()?;
    let tick_path = format!("/v1/bots/{}/tick", encode_path_segment(id));
    let mut ticks_sent = 0_u64;
    loop {
        let payload = api_post_json_with_client(&client, api_url, &tick_path).await?;
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
