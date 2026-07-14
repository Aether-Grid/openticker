use super::RUNTIME_BACKGROUND_POLL_INTERVAL_MS;
use crate::market_data::{
    LanePollPlan, LanePollingAdvance, PendingProviderEvent, StreamPollPlan,
    append_pending_provider_events,
};
use crate::{Runtime, ServiceError};
use openticker_dataplane::{DataPlane, StreamKey};
use openticker_gateway::Gateway;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub(super) async fn run_background_polling_loop(
    runtime: Arc<RwLock<Runtime>>,
    data_plane: Arc<DataPlane>,
    shutdown: watch::Receiver<bool>,
) {
    info!(
        interval_ms = RUNTIME_BACKGROUND_POLL_INTERVAL_MS,
        "starting runtime-owned background polling loop"
    );

    let mut shutdown = shutdown;
    loop {
        if *shutdown.borrow() {
            break;
        }

        let cycle_started_at = Instant::now();
        let now_ms = crate::unix_now_ms();
        let due_streams = data_plane.take_due_streams(now_ms);

        let (plans, gateway) = {
            let runtime = runtime.read().await;
            let gateway = runtime.connector_gateway_snapshot();
            let plans = due_streams
                .into_iter()
                .map(|stream_key| {
                    let plan = runtime.plan_stream_polling(&stream_key);
                    (stream_key, plan)
                })
                .collect::<Vec<_>>();
            (plans, gateway)
        };

        let throttled_accounts = gateway
            .statuses()
            .map(|statuses| {
                statuses
                    .into_iter()
                    .filter(|status| {
                        status
                            .resilience_state
                            .throttled_until_ms
                            .is_some_and(|until_ms| now_ms < until_ms)
                    })
                    .map(|status| status.account_id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();

        let mut join_set = JoinSet::<(StreamKey, Result<LanePollingAdvance, ServiceError>)>::new();
        for (stream_key, plan_result) in plans {
            if throttled_accounts.contains(&stream_key.account_id) {
                debug!(
                    account_id = %stream_key.account_id,
                    symbol = %stream_key.symbol,
                    timeframe = %stream_key.timeframe,
                    "skipping poll while provider rate limit is active"
                );
                continue;
            }
            let plan = match plan_result {
                Ok(plan) => plan,
                Err(error) => {
                    record_plan_phase_failure(&data_plane, &stream_key, &error);
                    // No fetch task is spawned for this stream, so the
                    // recorded plan-phase error cannot be overwritten by a
                    // later `record_fetched_bar` result within this cycle.
                    continue;
                }
            };

            let runtime = Arc::clone(&runtime);
            let data_plane = Arc::clone(&data_plane);
            let gateway = gateway.clone();
            join_set.spawn(async move {
                execute_stream_poll_cycle(runtime, data_plane, gateway, stream_key, plan).await
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            let (stream_key, advance_result) = match join_result {
                Ok(pair) => pair,
                Err(join_error) => {
                    error!(error = %join_error, "data-plane fetch task failed");
                    continue;
                }
            };
            record_stream_poll_result(&data_plane, &stream_key, now_ms, advance_result);
        }

        data_plane.record_cycle_duration(cycle_started_at.elapsed());

        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            () = sleep(Duration::from_millis(RUNTIME_BACKGROUND_POLL_INTERVAL_MS)) => {}
        }
    }

    info!("runtime-owned background polling loop stopped");
}

/// Records a plan-phase failure for a stream in the data plane. Callers must
/// skip spawning a fetch task for the stream afterwards so the recorded error
/// is not overwritten by a later fetch result within the same cycle.
fn record_plan_phase_failure(data_plane: &DataPlane, stream_key: &StreamKey, error: &ServiceError) {
    let error_message = error.to_string();
    if let Err(record_error) = data_plane.record_fetch_error(stream_key, &error_message) {
        warn!(
            account_id = %stream_key.account_id,
            symbol = %stream_key.symbol,
            timeframe = %stream_key.timeframe,
            error = %record_error,
            "failed to record plan-phase error"
        );
    }
    data_plane.record_completion(true);
    error!(
        account_id = %stream_key.account_id,
        symbol = %stream_key.symbol,
        timeframe = %stream_key.timeframe,
        error = %error_message,
        "data-plane plan phase failed"
    );
}

async fn execute_stream_poll_cycle(
    runtime: Arc<RwLock<Runtime>>,
    data_plane: Arc<DataPlane>,
    gateway: Gateway,
    stream_key: StreamKey,
    plan: StreamPollPlan,
) -> (StreamKey, Result<LanePollingAdvance, ServiceError>) {
    let result = match plan {
        StreamPollPlan::BareFetch {
            stream_id,
            account_id,
            account_kind,
            symbol,
            timeframe,
        } => {
            let error_stream_id = stream_id.clone();
            let error_account_id = account_id.clone();
            let fetch_started_at = Instant::now();
            // Connector fetches are blocking network I/O; run them off the
            // async workers so per-ticker polling cannot starve HTTP handlers.
            let execution = tokio::task::spawn_blocking(move || {
                Runtime::execute_bare_stream_fetch(
                    &gateway,
                    &stream_id,
                    &account_id,
                    &account_kind,
                    &symbol,
                    timeframe,
                )
            })
            .await;
            data_plane.record_connector_fetch_latency(fetch_started_at.elapsed());

            match execution {
                Ok(Ok(execution)) => {
                    let write_lock_wait_started_at = Instant::now();
                    let mut runtime = runtime.write().await;
                    data_plane.record_runtime_write_lock_wait(write_lock_wait_started_at.elapsed());
                    runtime.apply_bare_fetched_bar(
                        &stream_key,
                        execution.bar,
                        &execution.provider_events,
                    )
                }
                Ok(Err(failure)) => {
                    if let Err(error) =
                        flush_provider_events(&runtime, &failure.provider_events).await
                    {
                        Err(error)
                    } else {
                        Err(failure.error)
                    }
                }
                Err(join_error) => Err(ServiceError::DataConnectorUnavailable {
                    instance_id: error_stream_id,
                    account_id: error_account_id,
                    reason: describe_fetch_join_error(join_error),
                }),
            }
        }
        StreamPollPlan::LaneFanOut { instance_ids } => {
            execute_lane_fanout_poll_cycle(&runtime, &data_plane, &gateway, instance_ids).await
        }
    };

    (stream_key, result)
}

/// Renders a [`tokio::task::JoinError`] from a blocking fetch task into a
/// human-readable reason, distinguishing cancellation from panics and
/// extracting string panic payloads where possible.
pub(super) fn describe_fetch_join_error(join_error: tokio::task::JoinError) -> String {
    if join_error.is_cancelled() {
        return "blocking fetch task was cancelled".to_owned();
    }
    match join_error.try_into_panic() {
        Ok(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|message| (*message).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".to_owned());
            format!("blocking fetch task panicked: {message}")
        }
        Err(join_error) => format!("blocking fetch task failed: {join_error}"),
    }
}

async fn execute_lane_fanout_poll_cycle(
    runtime: &Arc<RwLock<Runtime>>,
    data_plane: &Arc<DataPlane>,
    gateway: &Gateway,
    instance_ids: Vec<String>,
) -> Result<LanePollingAdvance, ServiceError> {
    let mut bars_by_timestamp = BTreeMap::new();
    let mut outcomes = Vec::new();

    for instance_id in instance_ids {
        let mut next_plan = {
            let runtime = runtime.read().await;
            runtime.plan_lane_polling(
                instance_id.as_str(),
                crate::market_data::RECOVERY_PAGE_LIMIT,
                crate::unix_now_ms(),
            )?
        };
        let mut recovery_pages = 0usize;

        loop {
            let is_recovery_page = matches!(next_plan, LanePollPlan::ConfirmedRange { .. });
            if is_recovery_page
                && recovery_pages >= crate::market_data::MAX_RECOVERY_PAGES_PER_CYCLE.max(1)
            {
                break;
            }

            let (LanePollPlan::LatestBar { account_id, .. }
            | LanePollPlan::LatestConfirmedTarget { account_id, .. }
            | LanePollPlan::ConfirmedRange { account_id, .. }) = &next_plan;
            let error_account_id = account_id.clone();

            let fetch_started_at = Instant::now();
            // Connector fetches are blocking network I/O; run them off the
            // async workers so per-ticker polling cannot starve HTTP handlers.
            let fetch_gateway = gateway.clone();
            let (plan, execution) = tokio::task::spawn_blocking(move || {
                let execution = Runtime::execute_lane_poll_plan(&fetch_gateway, &next_plan);
                (next_plan, execution)
            })
            .await
            .map_err(|join_error| ServiceError::DataConnectorUnavailable {
                instance_id: instance_id.clone(),
                account_id: error_account_id,
                reason: describe_fetch_join_error(join_error),
            })?;
            data_plane.record_connector_fetch_latency(fetch_started_at.elapsed());

            let execution = match execution {
                Ok(execution) => execution,
                Err(failure) => {
                    flush_provider_events(runtime, &failure.provider_events).await?;
                    return Err(failure.error);
                }
            };

            let write_lock_wait_started_at = Instant::now();
            let mut runtime = runtime.write().await;
            data_plane.record_runtime_write_lock_wait(write_lock_wait_started_at.elapsed());
            let outcome = runtime.apply_lane_poll_plan(plan, execution)?;
            drop(runtime);

            if is_recovery_page {
                recovery_pages = recovery_pages.saturating_add(1);
            }
            for bar in outcome.advance.recorded_bars {
                bars_by_timestamp.insert(bar.timestamp, bar);
            }
            outcomes.extend(outcome.advance.outcomes);

            let Some(plan) = outcome.next_plan else {
                break;
            };
            next_plan = plan;
        }
    }

    Ok(LanePollingAdvance {
        recorded_bars: bars_by_timestamp.into_values().collect(),
        outcomes,
    })
}

async fn flush_provider_events(
    runtime: &Arc<RwLock<Runtime>>,
    events: &[PendingProviderEvent],
) -> Result<(), ServiceError> {
    if events.is_empty() {
        return Ok(());
    }
    let runtime = runtime.read().await;
    append_pending_provider_events(&runtime, events)
}

fn record_stream_poll_result(
    data_plane: &DataPlane,
    stream_key: &StreamKey,
    now_ms: i64,
    advance_result: Result<LanePollingAdvance, ServiceError>,
) {
    match advance_result {
        Ok(advance) => {
            let mut completion_error = None::<String>;
            for bar in advance.recorded_bars {
                if let Err(error) = data_plane.record_fetched_bar(stream_key, now_ms, bar) {
                    completion_error = Some(error.to_string());
                    break;
                }
            }

            if let Some(error_message) = completion_error {
                if let Err(e) = data_plane.record_fetch_error(stream_key, &error_message) {
                    tracing::warn!(
                        account_id = %stream_key.account_id,
                        symbol = %stream_key.symbol,
                        timeframe = %stream_key.timeframe,
                        error = %e,
                        "failed to record fetch error for stream bookkeeping"
                    );
                }
                data_plane.record_completion(true);
                error!(
                    account_id = %stream_key.account_id,
                    symbol = %stream_key.symbol,
                    timeframe = %stream_key.timeframe,
                    error = %error_message,
                    "data-plane apply phase failed"
                );
            } else {
                data_plane.record_completion(false);
            }
        }
        Err(error) => {
            let error_message = error.to_string();
            if let Err(e) = data_plane.record_fetch_error(stream_key, &error_message) {
                tracing::warn!(
                    account_id = %stream_key.account_id,
                    symbol = %stream_key.symbol,
                    timeframe = %stream_key.timeframe,
                    error = %e,
                    "failed to record fetch error for stream bookkeeping"
                );
            }
            data_plane.record_completion(true);
            error!(
                account_id = %stream_key.account_id,
                symbol = %stream_key.symbol,
                timeframe = %stream_key.timeframe,
                error = %error_message,
                "data-plane fetch cycle failed"
            );
        }
    }
}
