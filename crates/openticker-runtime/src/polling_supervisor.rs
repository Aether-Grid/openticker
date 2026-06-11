use crate::market_data::{
    LanePollPlan, LanePollingAdvance, PendingProviderEvent, StreamPollPlan,
    append_pending_provider_events,
};
use crate::{Runtime, ServiceError};
use openticker_connectors::{
    ConnectorMarketStreamSubscription, ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession,
    PreviewStreamConnectionState,
};
use openticker_dataplane::{DataPlane, StreamKey, StreamPreviewConnectionState};
use openticker_gateway::Gateway;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub const RUNTIME_BACKGROUND_POLL_INTERVAL_MS: u64 = 250;

/// Upper bound on concurrently active preview stream workers. Accounts beyond
/// this cap are skipped (with a warning) until capacity frees up, keeping the
/// worker map bounded even if configuration produces many preview-enabled
/// accounts.
const MAX_ACTIVE_PREVIEW_WORKERS: usize = 32;

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
fn describe_fetch_join_error(join_error: tokio::task::JoinError) -> String {
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

#[allow(clippy::too_many_lines)]
async fn run_background_preview_loop(
    runtime: Arc<RwLock<Runtime>>,
    data_plane: Arc<DataPlane>,
    shutdown: watch::Receiver<bool>,
) {
    info!("starting runtime-owned preview stream loop");

    let mut shutdown = shutdown;
    let mut workers = HashMap::<String, AccountPreviewWorker>::new();
    // Accounts already warned about the preview worker cap, so the warning is
    // emitted once per capacity episode instead of every loop iteration.
    let mut capacity_warned_accounts = HashSet::<String>::new();

    // Shutdown latency: the signal is observed at the top of each iteration,
    // once between the session-management and event-draining stages, and via
    // `shutdown.changed()` while sleeping between iterations. A shutdown
    // raised mid-stage is therefore only honored once the current stage
    // completes, giving a worst case of roughly one full iteration of work
    // plus the `RUNTIME_BACKGROUND_POLL_INTERVAL_MS` sleep. This is an
    // accepted trade-off; preview streams are non-critical for trading.
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

        let (admitted_accounts, capacity_skipped_accounts) =
            partition_preview_worker_candidates(&workers, &desired_by_account);
        capacity_warned_accounts
            .retain(|account_id| capacity_skipped_accounts.contains(&account_id));
        for account_id in capacity_skipped_accounts {
            if capacity_warned_accounts.insert(account_id.clone()) {
                warn!(
                    account_id = %account_id,
                    active_workers = workers.len(),
                    max_workers = MAX_ACTIVE_PREVIEW_WORKERS,
                    "preview worker limit reached; skipping preview stream for account"
                );
            }
        }

        for account_id in admitted_accounts {
            let subscriptions = &desired_by_account[account_id];
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

        // Re-check between processing stages so a shutdown raised while
        // sessions were being (re)started is honored before draining events.
        if *shutdown.borrow() {
            break;
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

/// Partitions the desired preview accounts that do not yet have a worker into
/// those admitted under [`MAX_ACTIVE_PREVIEW_WORKERS`] and those skipped
/// because the cap has been reached. Accounts that already have a worker are
/// excluded from both lists.
fn partition_preview_worker_candidates<'a, W>(
    workers: &HashMap<String, W>,
    desired_by_account: &'a BTreeMap<String, Vec<ConnectorMarketStreamSubscription>>,
) -> (Vec<&'a String>, Vec<&'a String>) {
    let mut admitted = Vec::new();
    let mut skipped = Vec::new();
    let mut projected_active = workers.len();
    for account_id in desired_by_account.keys() {
        if workers.contains_key(account_id) {
            continue;
        }
        if projected_active < MAX_ACTIVE_PREVIEW_WORKERS {
            projected_active += 1;
            admitted.push(account_id);
        } else {
            skipped.push(account_id);
        }
    }
    (admitted, skipped)
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
            if let Err(error) = data_plane.record_preview_update(&key, now_ms, update.bar.clone()) {
                warn!(
                    account_id = %key.account_id,
                    symbol = %key.symbol,
                    timeframe = %key.timeframe,
                    error = %error,
                    "failed to record preview update"
                );
            }
            if matches!(update.phase, crate::SignalPhase::Confirmed) {
                return;
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
        let key = StreamKey {
            account_id: account_id.to_owned(),
            symbol: subscription.symbol.clone(),
            timeframe: subscription.timeframe,
        };
        if let Err(error) =
            data_plane.record_preview_connection_state(&key, state, detail.map(str::to_owned))
        {
            warn!(
                account_id = %key.account_id,
                symbol = %key.symbol,
                timeframe = %key.timeframe,
                error = %error,
                "failed to record preview connection state"
            );
        }
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
    use crate::test_support::{fixture_bundle, test_bar_at};
    use crate::{NormalizedBarUpdate, SignalPhase, Timeframe};

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

    #[tokio::test]
    async fn preview_loop_ignores_confirmed_candle_close_updates() {
        let mut runtime = Runtime::from_config(&fixture_bundle());
        runtime.start_instance("aapl").expect("bot should start");
        let runtime = Arc::new(RwLock::new(runtime));
        let streams = {
            let runtime = runtime.read().await;
            runtime.effective_streams_for_dataplane()
        };
        let data_plane = Arc::new(DataPlane::new(streams));
        let subscription = ConnectorMarketStreamSubscription {
            symbol: "AAPL".to_owned(),
            timeframe: Timeframe::M1,
        };
        let previous_dispatched = runtime
            .read()
            .await
            .instance("aapl")
            .expect("lane should exist")
            .last_dispatched_bar_timestamp;

        handle_preview_event(
            &runtime,
            &data_plane,
            "alpaca-paper",
            std::slice::from_ref(&subscription),
            ConnectorPreviewStreamEvent::BarUpdate {
                subscription: subscription.clone(),
                update: NormalizedBarUpdate {
                    symbol: "AAPL".to_owned(),
                    phase: SignalPhase::Confirmed,
                    bar: test_bar_at("2030-01-01T00:01:00Z", 101.0),
                },
            },
        )
        .await;

        let stream = data_plane
            .snapshot_streams(crate::unix_now_ms(), 1)
            .remove(0);
        assert!(stream.latest_bar.is_none());
        let preview_close = stream.latest_preview_bar.unwrap().close;
        assert!((preview_close - 101.0).abs() < 1e-9);
        assert!(stream.last_preview_update_ms.is_some());
        assert_eq!(stream.last_confirmed_update_source, None);

        let lane = runtime
            .read()
            .await
            .instance("aapl")
            .expect("lane should exist")
            .last_dispatched_bar_timestamp;
        assert_eq!(lane, previous_dispatched);
    }

    #[test]
    fn preview_worker_admissions_respect_active_worker_cap() {
        let subscriptions = vec![ConnectorMarketStreamSubscription {
            symbol: "AAPL".to_owned(),
            timeframe: Timeframe::M1,
        }];

        // The value type stands in for `AccountPreviewWorker`; the admission
        // logic only inspects the keys and the map size.
        let mut workers = HashMap::<String, u8>::new();
        for index in 0..(MAX_ACTIVE_PREVIEW_WORKERS - 1) {
            workers.insert(format!("existing-{index:02}"), 0);
        }

        let mut desired = BTreeMap::<String, Vec<ConnectorMarketStreamSubscription>>::new();
        desired.insert("existing-00".to_owned(), subscriptions.clone());
        desired.insert("new-a".to_owned(), subscriptions.clone());
        desired.insert("new-b".to_owned(), subscriptions.clone());
        desired.insert("new-c".to_owned(), subscriptions);

        let (admitted, skipped) = partition_preview_worker_candidates(&workers, &desired);
        let admitted = admitted.into_iter().map(String::as_str).collect::<Vec<_>>();
        let skipped = skipped.into_iter().map(String::as_str).collect::<Vec<_>>();

        // One slot is free: the first missing account is admitted, the rest
        // are skipped, and accounts that already have a worker appear in
        // neither list.
        assert_eq!(admitted, ["new-a"]);
        assert_eq!(skipped, ["new-b", "new-c"]);
    }

    #[test]
    fn preview_worker_admissions_allow_all_when_under_cap() {
        let workers = HashMap::<String, u8>::new();
        let mut desired = BTreeMap::<String, Vec<ConnectorMarketStreamSubscription>>::new();
        desired.insert("acct-a".to_owned(), Vec::new());
        desired.insert("acct-b".to_owned(), Vec::new());

        let (admitted, skipped) = partition_preview_worker_candidates(&workers, &desired);
        assert_eq!(admitted.len(), 2);
        assert!(skipped.is_empty());
    }

    #[tokio::test]
    async fn describe_fetch_join_error_extracts_panic_payload() {
        let join_error = tokio::spawn(async { panic!("connector exploded") })
            .await
            .expect_err("task should panic");
        assert_eq!(
            describe_fetch_join_error(join_error),
            "blocking fetch task panicked: connector exploded"
        );
    }

    #[tokio::test]
    async fn describe_fetch_join_error_reports_cancellation() {
        let handle = tokio::spawn(std::future::pending::<()>());
        handle.abort();
        let join_error = handle.await.expect_err("task should be cancelled");
        assert_eq!(
            describe_fetch_join_error(join_error),
            "blocking fetch task was cancelled"
        );
    }
}
