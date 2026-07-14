use crate::Runtime;
use openticker_dataplane::DataPlane;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};
use tokio::task::JoinHandle;

mod preview;
mod stream_poll;

use preview::run_background_preview_loop;
use stream_poll::run_background_polling_loop;

pub const RUNTIME_BACKGROUND_POLL_INTERVAL_MS: u64 = 250;

#[derive(Debug)]
pub struct RuntimePollingSupervisor {
    shutdown_tx: watch::Sender<bool>,
    polling_task: JoinHandle<()>,
    preview_task: JoinHandle<()>,
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

#[cfg(test)]
mod tests {
    use super::preview::{
        MAX_ACTIVE_PREVIEW_WORKERS, handle_preview_event, partition_preview_worker_candidates,
    };
    use super::stream_poll::describe_fetch_join_error;
    use super::*;
    use crate::test_support::{fixture_bundle, test_bar_at};
    use crate::{NormalizedBarUpdate, SignalPhase, Timeframe};
    use openticker_connectors::{ConnectorMarketStreamSubscription, ConnectorPreviewStreamEvent};
    use std::collections::{BTreeMap, HashMap};

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
