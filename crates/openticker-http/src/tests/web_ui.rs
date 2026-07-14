use super::*;

#[tokio::test]
async fn dashboard_root_returns_html() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("OpenTicker"));
    assert!(html.contains("id=\"__nuxt\""));
}

#[tokio::test]
async fn dashboard_path_returns_html() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(DASHBOARD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("OpenTicker"));
}

#[tokio::test]
async fn dashboard_section_routes_return_html() {
    let app = build_router(fixture_state());
    let routes = [
        DASHBOARD_ACTIVITY_PATH,
        DASHBOARD_BOTS_PATH,
        "/bots/demo-bot",
        DASHBOARD_CONFIG_PATH,
        DASHBOARD_CONNECTORS_PATH,
        DASHBOARD_CYCLES_PATH,
        "/cycles/demo-bot/trc_test",
        DASHBOARD_FEEDS_PATH,
        "/feeds/alpaca/AAPL/1m",
        DASHBOARD_PROVIDERS_PATH,
        DASHBOARD_PORTFOLIO_PATH,
    ];

    for path in routes {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "path {path} did not return OK"
        );
        let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("OpenTicker"));
    }
}

#[tokio::test]
async fn unknown_frontend_path_falls_back_to_spa_shell() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/some/deep/spa/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("OpenTicker"));
}

#[tokio::test]
async fn nuxt_js_asset_served_with_javascript_mime() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/.output/public/_nuxt");
    let sample = std::fs::read_dir(&dir)
        .expect("ui/.output/public/_nuxt must exist — run `pnpm build` in ui/")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
        })
        .expect("at least one .js asset should be emitted by the Nuxt build");

    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/_nuxt/{sample}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        content_type.starts_with("application/javascript"),
        "expected javascript content-type for {sample}, got `{content_type}`"
    );
}

#[tokio::test]
async fn missing_nuxt_asset_returns_not_found() {
    let app = build_router(fixture_state());
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
async fn ui_asset_path_traversal_returns_not_found() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/_nuxt/../index.html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
