use super::*;

const TEST_API_TOKEN: &str = "secret-test-token";

#[tokio::test]
async fn api_returns_unauthorized_without_valid_token_when_auth_enabled() {
    let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(missing.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("bearer token")
    );

    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
                .header("authorization", "Bearer not-the-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let blocked_post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_post.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
                .header("authorization", format!("Bearer {TEST_API_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_exempts_probes_and_dashboard_assets() {
    let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

    let exempt_paths = [
        HEALTH_PATH,
        READY_PATH,
        METRICS_PATH,
        "/",
        DASHBOARD_PATH,
        "/favicon.ico",
        "/some/deep/spa/route",
    ];
    for path in exempt_paths {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "path {path} should be reachable without a token"
        );
    }

    // Asset routes are exempt as well: a missing asset must be a plain
    // 404, never a 401.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/_nuxt/does-not-exist.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_open_when_token_unset_or_empty() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let app = build_router_with_token(fixture_state(), Some(String::new()));
    let response = app
        .oneshot(
            Request::builder()
                .uri(SERVICE_STATUS_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn openapi_endpoint_requires_auth_when_token_is_set() {
    // /openapi.json is deliberately not in AUTH_EXEMPT_ROUTES — it exposes
    // the full API surface and should be gated behind the token.
    let app = build_router_with_token(fixture_state(), Some(TEST_API_TOKEN.to_owned()));

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(OPENAPI_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert!(
        unauthenticated
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .is_some(),
        "401 response must include WWW-Authenticate header"
    );

    let authorized = app
        .oneshot(
            Request::builder()
                .uri(OPENAPI_PATH)
                .header("authorization", format!("Bearer {TEST_API_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn oversized_request_body_is_rejected() {
    let app = build_router(fixture_state());
    let oversized_payload = format!(
        "{{\"padding\":\"{}\"}}",
        "x".repeat(crate::constants::MAX_REQUEST_BODY_BYTES + 1)
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bots/aapl/simulate-bar")
                .header("content-type", "application/json")
                .body(Body::from(oversized_payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn excessive_limit_query_parameter_is_clamped_and_succeeds() {
    let app = build_router(fixture_state());
    for uri in [
        "/v1/events?limit=999999999",
        "/v1/signals?limit=999999999",
        "/v1/orders?limit=999999999",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "uri {uri} should succeed with a clamped limit"
        );
    }
}
