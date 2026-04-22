use crate::Runtime;
use openticker_connectors::{
    ConnectorMarketStreamSubscription, ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession,
    PreviewStreamConnectionState,
};
use openticker_dataplane::{
    DataPlane, StreamKey, StreamPreviewConnectionState, StreamUpdateSource,
};
use openticker_gateway::Gateway;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::task::JoinHandle;
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
    pub fn start(runtime: Arc<RwLock<Runtime>>, data_plane: Arc<DataPlane>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let polling_task = tokio::spawn(run_background_polling_loop(
            Arc::clone(&runtime),
            Arc::clone(&data_plane),
            shutdown_rx,
        ));
        let preview_task = tokio::spawn(run_background_preview_loop(
            Arc::clone(&runtime),
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

        for stream_key in due_streams {
            let fetch_started_at = Instant::now();
            let write_lock_wait_started_at = Instant::now();
            let mut runtime = runtime.write().await;
            data_plane.record_runtime_write_lock_wait(write_lock_wait_started_at.elapsed());
            let advance_result = runtime.advance_stream_polling_once(&stream_key);
            drop(runtime);
            data_plane.record_connector_fetch_latency(fetch_started_at.elapsed());

            match advance_result {
                Ok(advance) => {
                    let mut completion_error = None::<String>;
                    for bar in advance.recorded_bars {
                        if let Err(error) = data_plane.record_fetched_bar(&stream_key, now_ms, bar)
                        {
                            completion_error = Some(error.to_string());
                            break;
                        }
                    }

                    if let Some(error_message) = completion_error {
                        let _ = data_plane.record_fetch_error(&stream_key, &error_message);
                        data_plane.record_completion(true);
                        error!(
                            account_id = %stream_key.account_id,
                            symbol = %stream_key.symbol,
                            timeframe = %stream_key.timeframe,
                            error = %error_message,
                            "data-plane recovery cycle failed"
                        );
                    } else {
                        data_plane.record_completion(false);
                    }
                }
                Err(error) => {
                    let error_message = error.to_string();
                    let _ = data_plane.record_fetch_error(&stream_key, &error_message);
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
                    Some("preview stream stopped".to_owned()),
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
                            Some(error),
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
                        Some("connector does not support preview streams".to_owned()),
                    );
                }
                Err(error) => {
                    record_preview_state_for_subscriptions(
                        &data_plane,
                        account_id,
                        subscriptions,
                        StreamPreviewConnectionState::Disconnected,
                        Some(error.to_string()),
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
                        Some(error),
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
                            Some("preview stream session ended".to_owned()),
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
            Some("preview stream shutdown".to_owned()),
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
                detail,
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
    detail: Option<String>,
) {
    for subscription in subscriptions {
        let _ = data_plane.record_preview_connection_state(
            &StreamKey {
                account_id: account_id.to_owned(),
                symbol: subscription.symbol.clone(),
                timeframe: subscription.timeframe,
            },
            state,
            detail.clone(),
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

        let supervisor = RuntimePollingSupervisor::start(runtime, data_plane);
        supervisor.shutdown().await;
    }
}
