use crate::market_data::{
    LanePollPlan, LanePollingAdvance, PendingProviderEvent, StreamPollPlan,
    append_pending_provider_events,
};
use crate::{Runtime, ServiceError};
use openticker_connectors::{
    ConnectorMarketStreamSubscription, ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession,
    PreviewStreamConnectionState,
};
use openticker_dataplane::{
    DataPlane, StreamKey, StreamPreviewConnectionState, StreamUpdateSource,
};
use openticker_gateway::Gateway;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use tracing::{error, info};

pub const RUNTIME_BACKGROUND_POLL_INTERVAL_MS: u64 = 250;

#[derive(Debug)]
pub struct RuntimePollingSupervisor {
    shutdown_tx: watch::Sender<bool>,
    polling_task: JoinHandle<()>,
    preview_task: JoinHandle<()>,
}

struct AccountPreviewWorker {
    subscriptions: Vec<ConnectorMarketStreamSubscription>,
    session: ConnectorPreviewStreamSession,
}

impl RuntimePollingSupervisor {
    #[must_use]
    pub fn start(runtime: &Arc<RwLock<Runtime>>, data_plane: Arc<DataPlane>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let polling_task = tokio::spawn(run_background_polling_loop(
            Arc::clone(runtime),
            Arc::clone(&data_plane),
            shutdown_rx,
        ));
        let preview_task = tokio::spawn(run_background_preview_loop(
            Arc::clone(runtime),
            data_plane,
            shutdown_tx.subscribe(),
        ));
        Self {
            shutdown_tx,
            polling_task,
            preview_task,
        }
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.polling_task.await;
        let _ = self.preview_task.await;
    }
}

async fn run_background_polling_loop(
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

        let mut join_set = JoinSet::<(StreamKey, Result<LanePollingAdvance, ServiceError>)>::new();
        for (stream_key, plan_result) in plans {
            let plan = match plan_result {
                Ok(plan) => plan,
                Err(error) => {
                    let error_message = error.to_string();
                    let _ = data_plane.record_fetch_error(&stream_key, &error_message);
                    data_plane.record_completion(true);
                    error!(
                        account_id = %stream_key.account_id,
                        symbol = %stream_key.symbol,
                        timeframe = %stream_key.timeframe,
                        error = %error_message,
                        "data-plane plan phase failed"
                    );
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
            let fetch_started_at = Instant::now();
            let execution = Runtime::execute_bare_stream_fetch(
                &gateway,
                &stream_id,
                &account_id,
                &account_kind,
                &symbol,
                timeframe,
            );
            data_plane.record_connector_fetch_latency(fetch_started_at.elapsed());

            match execution {
                Ok(execution) => {
                    let write_lock_wait_started_at = Instant::now();
                    let mut runtime = runtime.write().await;
                    data_plane.record_runtime_write_lock_wait(write_lock_wait_started_at.elapsed());
                    runtime.apply_bare_fetched_bar(
                        &stream_key,
                        execution.bar,
                        &execution.provider_events,
                    )
                }
                Err(failure) => {
                    if let Err(error) =
                        flush_provider_events(&runtime, &failure.provider_events).await
                    {
                        Err(error)
                    } else {
                        Err(failure.error)
                    }
                }
            }
        }
        StreamPollPlan::LaneFanOut { instance_ids } => {
            execute_lane_fanout_poll_cycle(&runtime, &data_plane, &gateway, instance_ids).await
        }
    };

    (stream_key, result)
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

            let fetch_started_at = Instant::now();
            let execution = Runtime::execute_lane_poll_plan(gateway, &next_plan);
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
            let outcome = runtime.apply_lane_poll_plan(next_plan, execution)?;
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
                let _ = data_plane.record_fetch_error(stream_key, &error_message);
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
            let _ = data_plane.record_fetch_error(stream_key, &error_message);
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

#[allow(clippy::too_many_lines)]
async fn run_background_preview_loop(
    runtime: Arc<RwLock<Runtime>>,
    data_plane: Arc<DataPlane>,
    shutdown: watch::Receiver<bool>,
) {
    info!("starting runtime-owned preview stream loop");

    let mut shutdown = shutdown;
    let mut workers = HashMap::<String, AccountPreviewWorker>::new();

    loop {
        if *shutdown.borrow() {
            break;
        }

        let (desired_by_account, gateway) = {
            let runtime = runtime.read().await;
            (
                runtime.effective_preview_streams_by_account(),
                Gateway::new(runtime.connector_registry()),
            )
        };

        let removed_accounts = workers
            .keys()
            .filter(|account_id| !desired_by_account.contains_key(*account_id))
            .cloned()
            .collect::<Vec<_>>();
        for account_id in removed_accounts {
            if let Some(worker) = workers.remove(&account_id) {
                let _ = worker.session.shutdown();
                record_preview_state_for_subscriptions(
                    &data_plane,
                    &account_id,
                    &worker.subscriptions,
                    StreamPreviewConnectionState::Disconnected,
                    Some("preview stream stopped"),
                );
            }
        }

        for (account_id, subscriptions) in &desired_by_account {
            if workers.contains_key(account_id) {
                continue;
            }

            match gateway.start_preview_stream_session(account_id) {
                Ok(Some(session)) => {
                    if let Err(error) = session.replace_subscriptions(subscriptions.clone()) {
                        record_preview_state_for_subscriptions(
                            &data_plane,
                            account_id,
                            subscriptions,
                            StreamPreviewConnectionState::Disconnected,
                            Some(error.as_str()),
                        );
                    } else {
                        workers.insert(
                            account_id.clone(),
                            AccountPreviewWorker {
                                subscriptions: subscriptions.clone(),
                                session,
                            },
                        );
                    }
                }
                Ok(None) => {
                    record_preview_state_for_subscriptions(
                        &data_plane,
                        account_id,
                        subscriptions,
                        StreamPreviewConnectionState::Disconnected,
                        Some("connector does not support preview streams"),
                    );
                }
                Err(error) => {
                    let error_message = error.to_string();
                    record_preview_state_for_subscriptions(
                        &data_plane,
                        account_id,
                        subscriptions,
                        StreamPreviewConnectionState::Disconnected,
                        Some(error_message.as_str()),
                    );
                }
            }
        }

        let mut restart_accounts = Vec::new();
        for (account_id, worker) in &mut workers {
            let desired = desired_by_account
                .get(account_id)
                .cloned()
                .unwrap_or_default();
            if worker.subscriptions != desired {
                if let Err(error) = worker.session.replace_subscriptions(desired.clone()) {
                    record_preview_state_for_subscriptions(
                        &data_plane,
                        account_id,
                        &desired,
                        StreamPreviewConnectionState::Disconnected,
                        Some(error.as_str()),
                    );
                    restart_accounts.push(account_id.clone());
                    continue;
                }
                worker.subscriptions = desired;
            }

            loop {
                match worker.session.try_recv() {
                    Ok(event) => {
                        handle_preview_event(
                            &runtime,
                            &data_plane,
                            account_id,
                            &worker.subscriptions,
                            event,
                        )
                        .await;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        record_preview_state_for_subscriptions(
                            &data_plane,
                            account_id,
                            &worker.subscriptions,
                            StreamPreviewConnectionState::Disconnected,
                            Some("preview stream session ended"),
                        );
                        restart_accounts.push(account_id.clone());
                        break;
                    }
                }
            }
        }

        for account_id in restart_accounts {
            workers.remove(&account_id);
        }

        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            () = sleep(Duration::from_millis(RUNTIME_BACKGROUND_POLL_INTERVAL_MS)) => {}
        }
    }

    for (account_id, worker) in workers {
        let _ = worker.session.shutdown();
        record_preview_state_for_subscriptions(
            &data_plane,
            &account_id,
            &worker.subscriptions,
            StreamPreviewConnectionState::Disconnected,
            Some("preview stream shutdown"),
        );
    }

    info!("runtime-owned preview stream loop stopped");
}

async fn handle_preview_event(
    runtime: &Arc<RwLock<Runtime>>,
    data_plane: &Arc<DataPlane>,
    account_id: &str,
    subscriptions: &[ConnectorMarketStreamSubscription],
    event: ConnectorPreviewStreamEvent,
) {
    match event {
        ConnectorPreviewStreamEvent::ConnectionState { state, detail } => {
            record_preview_state_for_subscriptions(
                data_plane,
                account_id,
                subscriptions,
                map_preview_state(state),
                detail.as_deref(),
            );
        }
        ConnectorPreviewStreamEvent::BarUpdate {
            subscription,
            update,
        } => {
            let key = StreamKey {
                account_id: account_id.to_owned(),
                symbol: subscription.symbol,
                timeframe: subscription.timeframe,
            };
            let now_ms = crate::unix_now_ms();
            let _ = data_plane.record_preview_update(&key, now_ms);
            if matches!(update.phase, crate::SignalPhase::Confirmed) {
                let _ = data_plane.record_fetched_bar_from_source(
                    &key,
                    now_ms,
                    update.bar.clone(),
                    StreamUpdateSource::PreviewStream,
                );
            }

            let mut runtime = runtime.write().await;
            if let Err(error) = runtime.process_market_stream_update_for_stream(&key, &update) {
                error!(
                    account_id = %key.account_id,
                    symbol = %key.symbol,
                    timeframe = %key.timeframe,
                    error = %error,
                    "preview stream update failed"
                );
            }
        }
    }
}

fn record_preview_state_for_subscriptions(
    data_plane: &DataPlane,
    account_id: &str,
    subscriptions: &[ConnectorMarketStreamSubscription],
    state: StreamPreviewConnectionState,
    detail: Option<&str>,
) {
    for subscription in subscriptions {
        let _ = data_plane.record_preview_connection_state(
            &StreamKey {
                account_id: account_id.to_owned(),
                symbol: subscription.symbol.clone(),
                timeframe: subscription.timeframe,
            },
            state,
            detail.map(str::to_owned),
        );
    }
}

fn map_preview_state(state: PreviewStreamConnectionState) -> StreamPreviewConnectionState {
    match state {
        PreviewStreamConnectionState::Connecting => StreamPreviewConnectionState::Connecting,
        PreviewStreamConnectionState::Connected => StreamPreviewConnectionState::Connected,
        PreviewStreamConnectionState::Disconnected => StreamPreviewConnectionState::Disconnected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_bundle;

    #[tokio::test]
    async fn polling_supervisor_starts_and_stops_cleanly() {
        let runtime = Arc::new(RwLock::new(Runtime::from_config(&fixture_bundle())));
        let streams = {
            let runtime = runtime.read().await;
            runtime.effective_streams_for_dataplane()
        };
        let data_plane = Arc::new(DataPlane::new(streams));

        let supervisor = RuntimePollingSupervisor::start(&runtime, data_plane);
        supervisor.shutdown().await;
    }
}
