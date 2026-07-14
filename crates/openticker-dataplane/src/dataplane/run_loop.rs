use super::{DataPlane, saturating_millis_to_i64};
use crate::stream::StreamKey;
use openticker_core::OhlcvBar;
use std::future::Future;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::time::sleep;

impl DataPlane {
    pub async fn run_forever<
        Attempt,
        AttemptFut,
        Fetch,
        FetchFut,
        Success,
        SuccessFut,
        ErrorCb,
        ErrorFut,
    >(
        &self,
        interval_ms: u64,
        mut shutdown: watch::Receiver<bool>,
        mut on_attempt: Attempt,
        mut fetcher: Fetch,
        mut on_success: Success,
        mut on_error: ErrorCb,
    ) where
        Attempt: FnMut(&StreamKey, i64) -> AttemptFut,
        AttemptFut: Future<Output = Result<(), String>>,
        Fetch: FnMut(&StreamKey) -> FetchFut,
        FetchFut: Future<Output = Result<OhlcvBar, String>>,
        Success: FnMut(&StreamKey, &OhlcvBar, i64, bool) -> SuccessFut,
        SuccessFut: Future<Output = Result<(), String>>,
        ErrorCb: FnMut(&StreamKey, &str) -> ErrorFut,
        ErrorFut: Future<Output = Result<(), String>>,
    {
        loop {
            if *shutdown.borrow() {
                break;
            }

            let cycle_started_at = Instant::now();
            let now_ms = unix_now_ms();
            let due_streams = self.take_due_streams(now_ms);

            for stream_key in due_streams {
                let attempt_result = on_attempt(&stream_key, now_ms).await;
                if let Err(error) = attempt_result {
                    // The stream may have been unregistered between selection
                    // and error handling; count any dropped failure so it is
                    // observable instead of vanishing, but keep the loop going.
                    if self.record_fetch_error(&stream_key, &error).is_err() {
                        self.record_dropped_error_record();
                    }
                    self.record_completion(true);
                    if on_error(&stream_key, &error).await.is_err() {
                        self.record_dropped_error_record();
                    }
                    continue;
                }

                let fetch_started_at = Instant::now();
                let fetched_bar = fetcher(&stream_key).await;
                self.record_connector_fetch_latency(fetch_started_at.elapsed());

                let completion_result = match fetched_bar {
                    Ok(bar) => match self.record_fetched_bar(&stream_key, now_ms, bar.clone()) {
                        Ok(appended) => on_success(&stream_key, &bar, now_ms, appended).await,
                        Err(error) => Err(error.to_string()),
                    },
                    Err(error) => {
                        // As above: surface dropped error records (e.g. for a
                        // concurrently unregistered stream) via a counter
                        // rather than discarding them silently.
                        if self.record_fetch_error(&stream_key, &error).is_err() {
                            self.record_dropped_error_record();
                        }
                        if on_error(&stream_key, &error).await.is_err() {
                            self.record_dropped_error_record();
                        }
                        Err(error)
                    }
                };

                self.record_completion(completion_result.is_err());
            }

            self.record_cycle_duration(cycle_started_at.elapsed());

            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                () = sleep(Duration::from_millis(interval_ms)) => {}
            }
        }
    }
}

fn unix_now_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    saturating_millis_to_i64(duration.as_millis())
}
