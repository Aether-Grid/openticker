mod common;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use openticker_http::{build_router, load_http_state};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tower::util::ServiceExt;

fn app_for(dir: &Path) -> axum::Router {
    let state = load_http_state(dir).expect("fixture config dir should load");
    build_router(state)
}

fn bot_body() -> Value {
    serde_json::to_value(common::fixture_bundle().instances[0].clone())
        .expect("instance should serialize")
}

fn account_update_body(total_budget_usd: f64) -> Value {
    json!({
        "kind": "alpaca",
        "mode": "paper",
        "use_demo_mode": false,
        "reconciliation_remote_snapshot": false,
        "execution_remote_submission": null,
        "reconciliation_base_url": null,
        "cash_balance_assets": [],
        "total_budget_usd": total_budget_usd,
    })
}

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: &Value,
    if_match: Option<&str>,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(generation) = if_match {
        builder = builder.header("if-match", generation);
    }
    let request = builder
        .body(Body::from(
            serde_json::to_vec(body).expect("body should serialize"),
        ))
        .expect("request should build");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("request should complete");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), common::MAX_TEST_RESPONSE_BYTES)
        .await
        .expect("body should be readable");
    (
        status,
        String::from_utf8(bytes.to_vec()).expect("body should be utf-8"),
    )
}

async fn send_empty(app: &axum::Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), common::MAX_TEST_RESPONSE_BYTES)
        .await
        .expect("body should be readable");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body should be JSON")
    };
    (status, body)
}

async fn get_json(app: &axum::Router, uri: &str) -> Value {
    let (status, body) = send_empty(app, "GET", uri).await;
    assert_eq!(status, StatusCode::OK, "GET {uri} failed: {body}");
    body
}

fn parse(body: &str) -> Value {
    serde_json::from_str(body).expect("response should be JSON")
}

fn violation_codes(body: &Value) -> Vec<String> {
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

#[tokio::test]
async fn put_bot_updates_disk_and_hot_applies() {
    let (_guard, dir) = common::fixture_config_dir("put-bot-happy");
    let app = app_for(&dir);

    let mut body = bot_body();
    body["polling_interval_ms"] = json!(2_000);
    let (status, response) = send_json(&app, "PUT", "/v1/config/bots/aapl", &body, None).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let response = parse(&response);
    assert_eq!(response["status"], "updated");
    assert_eq!(response["generation"], 1);
    assert_eq!(response["bot"]["polling_interval_ms"], 2_000);
    assert!(response["service"].is_object());

    let raw = fs::read_to_string(dir.join("bots").join("aapl.toml")).unwrap();
    let on_disk: toml::Value = toml::from_str(&raw).expect("written file should re-parse");
    assert_eq!(on_disk["polling_interval_ms"].as_integer(), Some(2_000));

    let effective = get_json(&app, "/v1/config/effective").await;
    assert_eq!(effective["bots"][0]["polling_interval_ms"], 2_000);

    let reload_status = get_json(&app, "/v1/config/reload-status").await;
    assert_eq!(reload_status["generation"], 1);
    assert_eq!(reload_status["last"]["trigger"], "config_write");
    assert_eq!(reload_status["last"]["outcome"], "reloaded");
}

#[tokio::test]
async fn put_bot_bundle_validation_failure_writes_nothing() {
    let (_guard, dir) = common::fixture_config_dir("put-bot-invalid");
    let app = app_for(&dir);
    let bot_path = dir.join("bots").join("aapl.toml");
    let before = fs::read_to_string(&bot_path).unwrap();

    let mut body = bot_body();
    body["polling_interval_ms"] = json!(0);
    let (status, response) = send_json(&app, "PUT", "/v1/config/bots/aapl", &body, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    let response = parse(&response);
    assert!(
        response["error"]
            .as_str()
            .unwrap_or_default()
            .contains("polling_interval_ms")
    );

    let after = fs::read_to_string(&bot_path).unwrap();
    assert_eq!(before, after, "file must be byte-identical after rejection");
}

#[tokio::test]
async fn put_bot_timeframe_change_for_running_instance_conflicts() {
    let (_guard, dir) = common::fixture_config_dir("put-bot-running");
    let app = app_for(&dir);
    let bot_path = dir.join("bots").join("aapl.toml");
    let before = fs::read_to_string(&bot_path).unwrap();

    let (start_status, _) = send_empty(&app, "POST", "/v1/bots/aapl/start").await;
    assert_eq!(start_status, StatusCode::OK);

    let mut body = bot_body();
    body["timeframe"] = json!("5m");
    let (status, response) = send_json(&app, "PUT", "/v1/config/bots/aapl", &body, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let response = parse(&response);
    assert!(
        violation_codes(&response).contains(&"timeframe_changed_running".to_owned()),
        "{response}"
    );

    let after = fs::read_to_string(&bot_path).unwrap();
    assert_eq!(before, after, "file must be unchanged after conflict");
}

#[tokio::test]
async fn put_account_with_secret_env_field_is_rejected() {
    let (_guard, dir) = common::fixture_config_dir("put-account-secret");
    let app = app_for(&dir);
    let account_path = dir.join("accounts").join("alpaca-paper.toml");
    let before = fs::read_to_string(&account_path).unwrap();

    let mut body = account_update_body(20_000.0);
    body["api_key_env"] = json!("PATH");
    let (status, response) =
        send_json(&app, "PUT", "/v1/config/accounts/alpaca-paper", &body, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");

    let after = fs::read_to_string(&account_path).unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn put_account_preserves_secrets_and_redacts_response() {
    let (_guard, dir) = common::fixture_config_dir("put-account-budget");
    let app = app_for(&dir);

    let body = account_update_body(25_000.0);
    let (status, response_text) =
        send_json(&app, "PUT", "/v1/config/accounts/alpaca-paper", &body, None).await;
    assert_eq!(status, StatusCode::OK, "{response_text}");

    // The on-disk account keeps its original secret env references untouched.
    let raw = fs::read_to_string(dir.join("accounts").join("alpaca-paper.toml")).unwrap();
    assert!(raw.contains("api_key_env = \"PATH\""), "{raw}");
    assert!(raw.contains("api_secret_env = \"PATH\""), "{raw}");
    let on_disk: toml::Value = toml::from_str(&raw).unwrap();
    assert_eq!(on_disk["total_budget_usd"].as_float(), Some(25_000.0));

    // The response never echoes env var names; it carries secret_status instead.
    assert!(!response_text.contains("api_key_env"), "{response_text}");
    assert!(!response_text.contains("api_secret_env"), "{response_text}");
    assert!(!response_text.contains("passphrase_env"), "{response_text}");
    let response = parse(&response_text);
    assert_eq!(response["status"], "updated");
    assert_eq!(response["account"]["total_budget_usd"], 25_000.0);
    assert!(response["account"]["secret_status"].is_object());
    assert_eq!(
        response["account"]["secret_status"]["api_key_present"],
        true
    );
}

#[tokio::test]
async fn put_account_budget_edit_echoing_resolved_remote_submission_succeeds() {
    // Reproduces the real dashboard flow: the account editor reads the resolved
    // `execution_remote_submission` bool from GET /v1/config/effective and echoes
    // it back on save (it cannot reconstruct the original `Option`). The fixture
    // account leaves `execution_remote_submission` unset (`None`) on disk, so the
    // resolved value is `false`. A budget-only edit must NOT be rejected as a
    // settings change, and must NOT materialize an `execution_remote_submission`
    // line into the account TOML.
    let (_guard, dir) = common::fixture_config_dir("put-account-resolved-remote");
    let app = app_for(&dir);
    let account_path = dir.join("accounts").join("alpaca-paper.toml");

    // Sanity: the fixture account starts with no materialized submission flag.
    let before = fs::read_to_string(&account_path).unwrap();
    assert!(
        !before.contains("execution_remote_submission"),
        "fixture account must start without a materialized submission flag: {before}"
    );

    // Read the resolved account exactly as the UI does.
    let effective = get_json(&app, "/v1/config/effective").await;
    let account = &effective["accounts"][0];
    let resolved_submission = account["execution_remote_submission"]
        .as_bool()
        .expect("effective account should carry a resolved submission bool");
    assert!(
        !resolved_submission,
        "fixture resolves submission to false: {account}"
    );

    // Build the save body the way the UI would: every field echoed back from the
    // effective account, with only the budget changed and the *resolved* bool
    // sent for execution_remote_submission.
    let body = json!({
        "kind": account["kind"],
        "mode": account["mode"],
        "use_demo_mode": account["use_demo_mode"],
        "reconciliation_remote_snapshot": account["reconciliation_remote_snapshot"],
        "execution_remote_submission": resolved_submission,
        "reconciliation_base_url": account["reconciliation_base_url"],
        "cash_balance_assets": account["cash_balance_assets"],
        "total_budget_usd": 31_000.0,
    });

    let (status, response_text) =
        send_json(&app, "PUT", "/v1/config/accounts/alpaca-paper", &body, None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "budget-only edit echoing the resolved submission bool must not 409: {response_text}"
    );

    // The budget actually changed on disk.
    let after = fs::read_to_string(&account_path).unwrap();
    let on_disk: toml::Value = toml::from_str(&after).unwrap();
    assert_eq!(on_disk["total_budget_usd"].as_float(), Some(31_000.0));

    // The budget-only edit must not have polluted the file with an inherited
    // default for execution_remote_submission.
    assert!(
        !after.contains("execution_remote_submission"),
        "budget-only edit must not materialize an inherited submission flag: {after}"
    );
}

#[tokio::test]
async fn put_account_unknown_id_returns_not_found() {
    let (_guard, dir) = common::fixture_config_dir("put-account-unknown");
    let app = app_for(&dir);

    let body = account_update_body(25_000.0);
    let (status, response) = send_json(&app, "PUT", "/v1/config/accounts/ghost", &body, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
}

#[tokio::test]
async fn post_bot_creates_file_and_conflicts_on_duplicate() {
    let (_guard, dir) = common::fixture_config_dir("post-bot-create");
    let app = app_for(&dir);

    let mut body = bot_body();
    body["id"] = json!("msft");
    body["symbols"] = json!(["MSFT"]);
    // Disabled so the account budget allocation (aapl already at 100%) stays valid.
    body["enabled"] = json!(false);

    let (status, response) = send_json(&app, "POST", "/v1/config/bots", &body, None).await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    let response = parse(&response);
    assert_eq!(response["status"], "created");
    assert_eq!(response["bot"]["id"], "msft");

    let raw = fs::read_to_string(dir.join("bots").join("msft.toml")).unwrap();
    let on_disk: toml::Value = toml::from_str(&raw).expect("created file should parse");
    assert_eq!(on_disk["id"].as_str(), Some("msft"));

    let effective = get_json(&app, "/v1/config/effective").await;
    assert_eq!(effective["bots"].as_array().map(Vec::len), Some(2));

    let (dup_status, dup_response) = send_json(&app, "POST", "/v1/config/bots", &body, None).await;
    assert_eq!(dup_status, StatusCode::CONFLICT, "{dup_response}");
    let dup_response = parse(&dup_response);
    assert!(dup_response["error"].as_str().is_some());
    assert!(
        violation_codes(&dup_response).contains(&"duplicate_bot".to_owned()),
        "{dup_response}"
    );
    assert_eq!(dup_response["violations"][0]["scope"], "bot:msft");
}

#[tokio::test]
async fn delete_stopped_bot_removes_file_and_config() {
    let (_guard, dir) = common::fixture_config_dir("delete-bot-stopped");
    let app = app_for(&dir);
    let bot_path = dir.join("bots").join("aapl.toml");

    let (status, response) = send_empty(&app, "DELETE", "/v1/config/bots/aapl").await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["status"], "deleted");
    assert_eq!(response["generation"], 1);
    assert!(response.get("bot").is_none());

    assert!(!bot_path.exists(), "bot file should be removed from disk");
    let effective = get_json(&app, "/v1/config/effective").await;
    assert_eq!(effective["bots"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn delete_running_bot_conflicts_and_keeps_file() {
    let (_guard, dir) = common::fixture_config_dir("delete-bot-running");
    let app = app_for(&dir);
    let bot_path = dir.join("bots").join("aapl.toml");

    let (start_status, _) = send_empty(&app, "POST", "/v1/bots/aapl/start").await;
    assert_eq!(start_status, StatusCode::OK);

    let (status, response) = send_empty(&app, "DELETE", "/v1/config/bots/aapl").await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert!(
        violation_codes(&response).contains(&"running_instance_removed".to_owned()),
        "{response}"
    );
    assert!(bot_path.exists(), "bot file must survive a rejected delete");
}

#[tokio::test]
async fn if_match_generation_guard_is_enforced() {
    let (_guard, dir) = common::fixture_config_dir("if-match");
    let app = app_for(&dir);
    let bot_path = dir.join("bots").join("aapl.toml");
    let before = fs::read_to_string(&bot_path).unwrap();

    let mut body = bot_body();
    body["polling_interval_ms"] = json!(3_000);

    let (malformed_status, malformed_response) = send_json(
        &app,
        "PUT",
        "/v1/config/bots/aapl",
        &body,
        Some("not-a-number"),
    )
    .await;
    assert_eq!(
        malformed_status,
        StatusCode::BAD_REQUEST,
        "{malformed_response}"
    );

    let (stale_status, stale_response) =
        send_json(&app, "PUT", "/v1/config/bots/aapl", &body, Some("999")).await;
    assert_eq!(stale_status, StatusCode::CONFLICT, "{stale_response}");
    let stale_response = parse(&stale_response);
    assert!(
        violation_codes(&stale_response).contains(&"stale_generation".to_owned()),
        "{stale_response}"
    );
    let after = fs::read_to_string(&bot_path).unwrap();
    assert_eq!(before, after, "stale write must not touch disk");

    let (fresh_status, fresh_response) =
        send_json(&app, "PUT", "/v1/config/bots/aapl", &body, Some("0")).await;
    assert_eq!(fresh_status, StatusCode::OK, "{fresh_response}");
    let fresh_response = parse(&fresh_response);
    assert_eq!(fresh_response["generation"], 1);
}

#[tokio::test]
async fn concurrent_puts_serialize_without_server_errors() {
    let (_guard, dir) = common::fixture_config_dir("concurrent-puts");
    let app = app_for(&dir);

    let mut body_a = bot_body();
    body_a["polling_interval_ms"] = json!(1_500);
    let mut body_b = bot_body();
    body_b["polling_enabled"] = json!(false);

    let (result_a, result_b) = tokio::join!(
        send_json(&app, "PUT", "/v1/config/bots/aapl", &body_a, None),
        send_json(&app, "PUT", "/v1/config/bots/aapl", &body_b, None),
    );

    for (status, response) in [&result_a, &result_b] {
        assert!(
            status.is_success() || *status == StatusCode::CONFLICT,
            "concurrent write returned {status}: {response}"
        );
    }

    let raw = fs::read_to_string(dir.join("bots").join("aapl.toml")).unwrap();
    let _: toml::Value = toml::from_str(&raw).expect("final file should parse");

    let effective = get_json(&app, "/v1/config/effective").await;
    let final_bot = &effective["bots"][0];
    assert!(
        *final_bot == body_a || *final_bot == body_b,
        "final config should equal exactly one of the writes, got {final_bot}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contended_parallel_puts_have_no_server_errors() {
    let (_guard, dir) = common::fixture_config_dir("contended-puts");
    let app = app_for(&dir);

    let intervals: [u64; 4] = [1_100, 1_200, 1_300, 1_400];
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(intervals.len()));

    let handles = intervals
        .into_iter()
        .map(|interval| {
            let app = app.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            tokio::spawn(async move {
                let mut body = bot_body();
                body["polling_interval_ms"] = json!(interval);
                // Release every writer at once to maximize real contention.
                barrier.wait().await;
                let (status, response) =
                    send_json(&app, "PUT", "/v1/config/bots/aapl", &body, None).await;
                (interval, status, response)
            })
        })
        .collect::<Vec<_>>();

    let mut successes = 0;
    for handle in handles {
        let (interval, status, response) = handle.await.expect("writer task should not panic");
        assert!(
            !status.is_server_error(),
            "concurrent write for interval {interval} returned {status}: {response}"
        );
        if status.is_success() {
            successes += 1;
        }
    }
    assert!(successes > 0, "at least one concurrent write must succeed");

    let raw = fs::read_to_string(dir.join("bots").join("aapl.toml")).unwrap();
    let on_disk: toml::Value = toml::from_str(&raw).expect("final file should parse");
    let on_disk_interval = on_disk["polling_interval_ms"]
        .as_integer()
        .and_then(|value| u64::try_from(value).ok())
        .expect("final file should carry a polling interval");

    let effective = get_json(&app, "/v1/config/effective").await;
    let effective_interval = effective["bots"][0]["polling_interval_ms"]
        .as_u64()
        .expect("effective bot should carry a polling interval");

    assert_eq!(
        on_disk_interval, effective_interval,
        "disk and effective state must agree after contended writes"
    );
    assert!(
        intervals.contains(&effective_interval),
        "final effective state must equal one of the writes, got {effective_interval}"
    );
}

#[tokio::test]
async fn hot_apply_of_allowed_change_preserves_running_bot() {
    let (_guard, dir) = common::fixture_config_dir("hot-apply-running");
    let app = app_for(&dir);

    let (start_status, start_body) = send_empty(&app, "POST", "/v1/bots/aapl/start").await;
    assert_eq!(start_status, StatusCode::OK, "{start_body}");
    assert_eq!(start_body["state"], "running", "{start_body}");

    let mut body = bot_body();
    body["polling_interval_ms"] = json!(2_500);
    let (status, response) = send_json(&app, "PUT", "/v1/config/bots/aapl", &body, None).await;
    assert_eq!(status, StatusCode::OK, "{response}");

    let bot = get_json(&app, "/v1/bots/aapl").await;
    assert_eq!(
        bot["state"], "running",
        "hot apply must not stop the running bot: {bot}"
    );
    assert_eq!(bot["polling"]["interval_ms"], 2_500, "{bot}");
}

#[tokio::test]
async fn put_global_storage_path_change_conflicts_and_keeps_file() {
    let (_guard, dir) = common::fixture_config_dir("put-global-storage");
    let app = app_for(&dir);
    let global_path = dir.join("openticker.toml");
    let before = fs::read_to_string(&global_path).unwrap();

    let effective = get_json(&app, "/v1/config/effective").await;
    let mut global = effective["global"].clone();
    global["storage"]["path"] = json!(dir.join("elsewhere.db").to_string_lossy());

    let (status, response) = send_json(&app, "PUT", "/v1/config/global", &global, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let response = parse(&response);
    assert!(
        violation_codes(&response).contains(&"storage_changed".to_owned()),
        "{response}"
    );

    let after = fs::read_to_string(&global_path).unwrap();
    assert_eq!(
        before, after,
        "global file must be unchanged after a storage-change conflict"
    );
}

#[tokio::test]
async fn reload_success_bodies_carry_generation() {
    let (_guard, dir) = common::fixture_config_dir("reload-generation");
    let app = app_for(&dir);

    // The service was loaded from this directory, so the first reload is a
    // no-op; the body must still carry the generation for If-Match bootstrap.
    let (status, body) = send_empty(&app, "POST", "/v1/config/reload").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "no_change");
    assert_eq!(body["generation"], 0, "{body}");

    // A config write bumps the generation; the next no-op reload reports it.
    let mut bot = bot_body();
    bot["polling_interval_ms"] = json!(2_000);
    let (put_status, put_response) =
        send_json(&app, "PUT", "/v1/config/bots/aapl", &bot, None).await;
    assert_eq!(put_status, StatusCode::OK, "{put_response}");

    let (status, body) = send_empty(&app, "POST", "/v1/config/reload").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "no_change");
    assert_eq!(body["generation"], 1, "{body}");

    // Mutate the file on disk behind the service's back so the next reload
    // actually applies and reports the bumped generation.
    let mut updated = common::fixture_bundle().instances[0].clone();
    updated.polling_interval_ms = 3_000;
    fs::write(
        dir.join("bots").join("aapl.toml"),
        openticker_config::render_new_document(&updated).expect("instance should render"),
    )
    .unwrap();

    let (status, body) = send_empty(&app, "POST", "/v1/config/reload").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "reloaded");
    assert_eq!(body["generation"], 2, "{body}");
}

#[tokio::test]
async fn put_bot_id_mismatch_unknown_id_and_unsafe_create_id() {
    let (_guard, dir) = common::fixture_config_dir("bot-id-guards");
    let app = app_for(&dir);

    // Body id != path id.
    let body = bot_body();
    let (mismatch_status, mismatch_response) =
        send_json(&app, "PUT", "/v1/config/bots/other", &body, None).await;
    assert_eq!(
        mismatch_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{mismatch_response}"
    );

    // Unknown bot id.
    let mut ghost = bot_body();
    ghost["id"] = json!("ghost");
    let (unknown_status, unknown_response) =
        send_json(&app, "PUT", "/v1/config/bots/ghost", &ghost, None).await;
    assert_eq!(unknown_status, StatusCode::NOT_FOUND, "{unknown_response}");

    // Path-traversal create attempt.
    let mut evil = bot_body();
    evil["id"] = json!("../evil");
    let (evil_status, evil_response) =
        send_json(&app, "POST", "/v1/config/bots", &evil, None).await;
    assert_eq!(
        evil_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{evil_response}"
    );
    assert!(!dir.join("evil.toml").exists());
    assert!(!dir.parent().unwrap().join("evil.toml").exists());
}

#[tokio::test]
async fn post_bot_with_overlong_id_is_rejected_early() {
    let (_guard, dir) = common::fixture_config_dir("bot-id-overlong");
    let app = app_for(&dir);

    let long_id = "a".repeat(300);
    let mut body = bot_body();
    body["id"] = json!(long_id.clone());
    body["enabled"] = json!(false);

    let (status, response) = send_json(&app, "POST", "/v1/config/bots", &body, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");

    assert!(
        !dir.join("bots").join(format!("{long_id}.toml")).exists(),
        "no file may be created for a rejected oversized id"
    );
    let bot_files = fs::read_dir(dir.join("bots"))
        .expect("bots dir should be readable")
        .count();
    assert_eq!(bot_files, 1, "only the fixture bot file should exist");
}

#[tokio::test]
async fn put_global_updates_document() {
    let (_guard, dir) = common::fixture_config_dir("put-global");
    let app = app_for(&dir);

    let effective = get_json(&app, "/v1/config/effective").await;
    let mut global = effective["global"].clone();
    global["observability"]["log_level"] = json!("debug");

    let (status, response) = send_json(&app, "PUT", "/v1/config/global", &global, None).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let response = parse(&response);
    assert_eq!(response["status"], "updated");
    assert_eq!(response["global"]["observability"]["log_level"], "debug");

    let raw = fs::read_to_string(dir.join("openticker.toml")).unwrap();
    assert!(raw.contains("log_level = \"debug\""), "{raw}");
}

#[tokio::test]
async fn put_risk_profile_updates_document_and_guards_ids() {
    let (_guard, dir) = common::fixture_config_dir("put-risk");
    let app = app_for(&dir);

    let mut profile =
        serde_json::to_value(common::fixture_bundle().risk_profiles[0].clone()).unwrap();
    profile["max_open_positions"] = json!(3);

    let (status, response) = send_json(
        &app,
        "PUT",
        "/v1/config/risk-profiles/equities-default",
        &profile,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let response = parse(&response);
    assert_eq!(response["risk_profile"]["max_open_positions"], 3);

    let raw = fs::read_to_string(dir.join("risk").join("equities-default.toml")).unwrap();
    let on_disk: toml::Value = toml::from_str(&raw).unwrap();
    assert_eq!(on_disk["max_open_positions"].as_integer(), Some(3));

    let (mismatch_status, _) = send_json(
        &app,
        "PUT",
        "/v1/config/risk-profiles/other",
        &profile,
        None,
    )
    .await;
    assert_eq!(mismatch_status, StatusCode::UNPROCESSABLE_ENTITY);

    profile["id"] = json!("ghost");
    let (unknown_status, _) = send_json(
        &app,
        "PUT",
        "/v1/config/risk-profiles/ghost",
        &profile,
        None,
    )
    .await;
    assert_eq!(unknown_status, StatusCode::NOT_FOUND);
}
