mod common;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use openticker_http::{HttpState, build_router};
use openticker_runtime::Runtime;
use tower::util::ServiceExt;

#[tokio::test]
async fn status_payload_is_stable() {
    let bundle = common::fixture_bundle();
    let runtime = Runtime::from_config(&bundle);
    let state = HttpState::new(runtime);
    let router = build_router(state);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/v1/service/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), common::MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value.get("totalInstances").is_some() || value.get("total_instances").is_some());
}
