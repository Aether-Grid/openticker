mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use openticker_http::{HttpState, build_router};
use openticker_runtime::{Runtime, RuntimePollingSupervisor};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

async fn measure_latency(router: axum::Router, path: &str, samples: usize) -> Vec<Duration> {
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        durations.push(start.elapsed());
    }
    durations.sort();
    durations
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[idx]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual performance baseline"]
async fn perf_baseline_status_and_bots() {
    for bot_count in [10usize, 50, 100] {
        let bundle = common::synthetic_bundle(bot_count);
        let runtime = Arc::new(RwLock::new(Runtime::from_config(&bundle)));
        let streams = runtime.read().await.effective_streams_for_dataplane();
        let data_plane = Arc::new(openticker_dataplane::DataPlane::new(streams));
        let state = HttpState::new_from_parts(Arc::clone(&runtime), Arc::clone(&data_plane));
        let router = build_router(state);
        let supervisor = RuntimePollingSupervisor::start(&runtime, data_plane);

        tokio::time::sleep(Duration::from_millis(500)).await;

        let status = measure_latency(router.clone(), "/v1/service/status", 50).await;
        let bots = measure_latency(router.clone(), "/v1/bots", 50).await;

        println!(
            "bot_count={bot_count} status p50={:?} p95={:?} p99={:?}",
            percentile(&status, 0.50),
            percentile(&status, 0.95),
            percentile(&status, 0.99),
        );
        println!(
            "bot_count={bot_count} bots   p50={:?} p95={:?} p99={:?}",
            percentile(&bots, 0.50),
            percentile(&bots, 0.95),
            percentile(&bots, 0.99),
        );

        supervisor.shutdown().await;
    }
}
