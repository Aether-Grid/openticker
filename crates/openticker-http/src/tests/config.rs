use super::*;

#[tokio::test]
async fn config_effective_returns_not_implemented_without_bundle() {
    let app = build_router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri(CONFIG_EFFECTIVE_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn config_effective_returns_payload_with_managed_bundle() {
    let app = build_router(fixture_state_with_config());
    let response = app
        .oneshot(
            Request::builder()
                .uri(CONFIG_EFFECTIVE_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["bots"].is_array());
}

#[tokio::test]
async fn config_effective_redacts_secret_field_names() {
    let mut bundle = fixture_bundle();
    bundle.accounts[0].api_key_env = Some("OPENTICKER_HTTP_API_KEY".to_owned());
    bundle.accounts[0].api_secret_env = Some("OPENTICKER_HTTP_API_SECRET".to_owned());
    let app = build_router(HttpState::with_config(
        Runtime::from_config(&bundle),
        PathBuf::from("./config"),
        bundle,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri(CONFIG_EFFECTIVE_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&body_text).unwrap();
    assert!(json["accounts"][0]["secret_status"].is_object());
    assert!(!body_text.contains("api_key_env"));
    assert!(!body_text.contains("api_secret_env"));
    assert!(!body_text.contains("OPENTICKER_HTTP_API_KEY"));
    assert!(!body_text.contains("OPENTICKER_HTTP_API_SECRET"));
}

#[tokio::test]
async fn config_reload_succeeds_for_valid_managed_config() {
    let config_dir = create_managed_config_dir("reload-valid");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir, bundle));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_reload_applies_polling_interval_change() {
    let config_dir = create_managed_config_dir("reload-polling-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

    let reload_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reload_response.status(), StatusCode::OK);

    let instance_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/bots/aapl")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(instance_response.status(), StatusCode::OK);
    let body = to_bytes(instance_response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["polling"]["enabled"], true);
    assert_eq!(json["polling"]["interval_ms"], 250);
    assert_eq!(json["position"]["has_position"], false);
    assert_eq!(json["position"]["quantity"], 0.0);
    assert!(json["position"]["entry_price"].is_null());
}

#[tokio::test]
async fn config_reload_rejects_invalid_connector_binding() {
    let config_dir = create_managed_config_dir("reload-invalid");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_config(&config_dir, &storage_path, "kraken", "1m");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("execution_connector"));
}

#[tokio::test]
async fn config_reload_rejects_storage_path_change() {
    let config_dir = create_managed_config_dir("reload-storage-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    let changed_storage_path = config_dir.join("runtime-next.db");
    write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("storage"));
    assert_eq!(json["violations"][0]["code"], "storage_changed");
    assert_eq!(json["violations"][0]["scope"], "global");
}

#[tokio::test]
async fn config_reload_rejects_account_credential_reference_change() {
    let config_dir = create_managed_config_dir("reload-credential-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    let alternate_secret_env = existing_env_var_name_except("PATH");
    write_managed_account(&config_dir, "PATH", alternate_secret_env);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("credential references"));
    assert_eq!(json["violations"][0]["code"], "credentials_changed");
    assert_eq!(json["violations"][0]["scope"], "account:alpaca-paper");
}

#[tokio::test]
async fn config_reload_rejects_account_reconciliation_settings_change() {
    let config_dir = create_managed_config_dir("reload-reconcile-settings-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_account_with_reconciliation(
        &config_dir,
        "PATH",
        "PATH",
        true,
        Some("https://paper-api.alpaca.markets"),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("connector settings"));
    assert_eq!(json["violations"][0]["code"], "account_settings_changed");
}

#[tokio::test]
async fn config_reload_rejects_timeframe_change_for_running_instance() {
    let config_dir = create_managed_config_dir("reload-running-timeframe-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    runtime.start_instance("aapl").unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_config(&config_dir, &storage_path, "alpaca", "5m");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("timeframe"));
    assert_eq!(json["violations"][0]["code"], "timeframe_changed_running");
    assert_eq!(json["violations"][0]["scope"], "bot:aapl");
}

#[tokio::test]
async fn config_reload_rejects_symbol_change_for_running_instance() {
    let config_dir = create_managed_config_dir("reload-running-symbol-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    runtime.start_instance("aapl").unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_instance_with_symbol(&config_dir, "MSFT", "alpaca", "1m", None, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("symbols changed"));
    assert_eq!(json["violations"][0]["code"], "symbols_changed_running");
}

#[tokio::test]
async fn config_reload_rejects_removing_running_instance() {
    let config_dir = create_managed_config_dir("reload-running-instance-removed");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let mut runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    runtime.start_instance("aapl").unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    fs::remove_file(config_dir.join("bots").join("aapl.toml"))
        .expect("managed instance should be removed");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = json["error"].as_str().unwrap_or_default();
    assert!(error.contains("running instance `aapl` was removed"));
    assert_eq!(json["violations"][0]["code"], "running_instance_removed");
}

#[tokio::test]
async fn config_reload_status_starts_with_generation_zero_and_empty_history() {
    let app = build_router(fixture_state());

    let json = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(json["generation"], 0);
    assert!(json["last"].is_null());
    assert_eq!(json["history"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn config_reload_status_tracks_reloaded_and_no_change_outcomes() {
    let config_dir = create_managed_config_dir("reload-status-track");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

    let (status, body) = post_reload(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "reloaded");
    assert_eq!(body["reloaded"], true);

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(reload_status["generation"], 1);
    assert_eq!(reload_status["last"]["outcome"], "reloaded");
    assert_eq!(reload_status["last"]["trigger"], "manual_api");
    assert_eq!(reload_status["last"]["generation"], 1);
    assert!(reload_status["last"]["error"].is_null());

    let (second_status, second_body) = post_reload(&app).await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(second_body["status"], "no_change");
    assert_eq!(second_body["reloaded"], false);

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(reload_status["generation"], 1);
    assert_eq!(reload_status["last"]["outcome"], "no_change");
    assert_eq!(reload_status["last"]["trigger"], "manual_api");
    assert_eq!(reload_status["last"]["generation"], 1);
    assert_eq!(reload_status["history"].as_array().map(Vec::len), Some(2));
    assert_eq!(reload_status["history"][0]["outcome"], "no_change");
    assert_eq!(reload_status["history"][1]["outcome"], "reloaded");
}

#[tokio::test]
async fn config_reload_rejection_records_structured_violations_in_status() {
    let config_dir = create_managed_config_dir("reload-status-rejected");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    let changed_storage_path = config_dir.join("runtime-next.db");
    write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");

    let (status, body) = post_reload(&app).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(violation_codes(&body).contains(&"storage_changed".to_owned()));

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(reload_status["generation"], 0);
    assert_eq!(reload_status["last"]["outcome"], "rejected");
    assert_eq!(reload_status["last"]["trigger"], "manual_api");
    assert_eq!(
        reload_status["last"]["violations"][0]["code"],
        "storage_changed"
    );
}

#[tokio::test]
async fn config_reload_collects_all_change_set_violations() {
    let config_dir = create_managed_config_dir("reload-multi-violation");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    let changed_storage_path = config_dir.join("runtime-next.db");
    write_managed_config(&config_dir, &changed_storage_path, "alpaca", "1m");
    let alternate_secret_env = existing_env_var_name_except("PATH");
    write_managed_account(&config_dir, "PATH", alternate_secret_env);

    let (status, body) = post_reload(&app).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let codes = violation_codes(&body);
    assert!(codes.contains(&"storage_changed".to_owned()), "{codes:?}");
    assert!(
        codes.contains(&"credentials_changed".to_owned()),
        "{codes:?}"
    );
}

#[tokio::test]
async fn config_reload_rejects_bot_dir_change() {
    let config_dir = create_managed_config_dir("reload-bot-dir-change");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    // Same instance files in a different directory; only service.bot_dir changes.
    fs::create_dir_all(config_dir.join("bots-next")).expect("bots-next dir should be created");
    fs::copy(
        config_dir.join("bots").join("aapl.toml"),
        config_dir.join("bots-next").join("aapl.toml"),
    )
    .expect("instance config should be copied");
    write_managed_global(&config_dir, &storage_path, "./bots-next");

    let (status, body) = post_reload(&app).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["violations"][0]["code"], "bot_dir_changed");
    assert_eq!(body["violations"][0]["scope"], "global");
}

#[tokio::test]
async fn config_reload_status_history_truncates_to_limit_newest_first() {
    let config_dir = create_managed_config_dir("reload-status-cap");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    let limit = crate::config_ops::RELOAD_STATUS_HISTORY_LIMIT;
    let cycles = u64::try_from(limit).unwrap() + 5;
    for cycle in 0..cycles {
        write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(200 + cycle));
        let (status, body) = post_reload(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["reloaded"], true,
            "cycle {cycle} should apply a changed config"
        );
    }

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(reload_status["generation"], cycles);
    let history = reload_status["history"].as_array().unwrap();
    assert_eq!(history.len(), limit);
    assert_eq!(history[0]["generation"], cycles);
    assert_eq!(
        history[limit - 1]["generation"],
        cycles - u64::try_from(limit).unwrap() + 1
    );
    assert!(history.iter().all(|entry| entry["outcome"] == "reloaded"));
}

#[tokio::test]
async fn config_reload_records_failed_outcome_for_corrupt_toml() {
    let config_dir = create_managed_config_dir("reload-corrupt-toml");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    fs::write(
        config_dir.join("openticker.toml"),
        "this is not [valid toml",
    )
    .expect("corrupt global config should be written");

    let (status, body) = post_reload(&app).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("config reload failed")
    );

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    assert_eq!(reload_status["generation"], 0);
    assert_eq!(reload_status["last"]["outcome"], "failed");
    assert_eq!(reload_status["last"]["trigger"], "manual_api");
    assert!(reload_status["last"]["error"].as_str().is_some());
}

#[tokio::test]
async fn concurrent_config_reloads_keep_generation_and_history_coherent() {
    let config_dir = create_managed_config_dir("reload-concurrent");
    let storage_path = config_dir.join("runtime.db");
    write_managed_config(&config_dir, &storage_path, "alpaca", "1m");

    let bundle = load_from_dir(&config_dir).unwrap();
    let runtime = Runtime::from_config_with_storage(&bundle).unwrap();
    let app = build_router(HttpState::with_config(runtime, config_dir.clone(), bundle));

    write_managed_instance(&config_dir, "alpaca", "1m", Some(true), Some(250));

    let responses = tokio::join!(
        post_reload(&app),
        post_reload(&app),
        post_reload(&app),
        post_reload(&app)
    );
    let responses = [responses.0, responses.1, responses.2, responses.3];

    let mut applied_count = 0u64;
    for (status, body) in &responses {
        assert!(status.is_success(), "concurrent reload returned {status}");
        if body["reloaded"] == true {
            applied_count += 1;
        }
    }
    assert!(applied_count >= 1);

    let reload_status = get_json(&app, CONFIG_RELOAD_STATUS_PATH).await;
    let final_generation = reload_status["generation"].as_u64().unwrap();
    assert_eq!(final_generation, applied_count);

    let history = reload_status["history"].as_array().unwrap();
    assert_eq!(history.len(), responses.len());
    let reloaded_entries = history
        .iter()
        .filter(|entry| entry["outcome"] == "reloaded")
        .count();
    assert_eq!(u64::try_from(reloaded_entries).unwrap(), applied_count);
    assert!(
        reloaded_entries <= 1,
        "a single distinct config state should apply at most once"
    );
    assert!(
        history
            .iter()
            .all(|entry| entry["generation"].as_u64().unwrap() <= final_generation)
    );
}

async fn post_reload(app: &axum::Router) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CONFIG_RELOAD_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), MAX_TEST_RESPONSE_BYTES)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap();
    (status, json)
}

fn violation_codes(body: &serde_json::Value) -> Vec<String> {
    body["violations"]
        .as_array()
        .map(|violations| {
            violations
                .iter()
                .filter_map(|violation| violation["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
