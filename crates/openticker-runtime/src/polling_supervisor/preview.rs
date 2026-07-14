use super::RUNTIME_BACKGROUND_POLL_INTERVAL_MS;
use crate::Runtime;
use openticker_connectors::{
    ConnectorMarketStreamSubscription, ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession,
    PreviewStreamConnectionState,
};
use openticker_dataplane::{DataPlane, StreamKey, StreamPreviewConnectionState};
use openticker_gateway::Gateway;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, watch};
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Upper bound on concurrently active preview stream workers. Accounts beyond
/// this cap are skipped (with a warning) until capacity frees up, keeping the
/// worker map bounded even if configuration produces many preview-enabled
/// accounts.
pub(super) const MAX_ACTIVE_PREVIEW_WORKERS: usize = 32;

struct AccountPreviewWorker {
    subscriptions: Vec<ConnectorMarketStreamSubscription>,
    session: ConnectorPreviewStreamSession,
}

#[allow(clippy::too_many_lines)]
pub(super) async fn run_background_preview_loop(
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
pub(super) fn partition_preview_worker_candidates<'a, W>(
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

pub(super) async fn handle_preview_event(
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
