//! Shared helpers for gateway integration tests.

use openticker_config::AccountConfig;
use openticker_core::ExecutionMode;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_millis(),
    )
    .expect("timestamp should fit in i64")
}

pub(crate) fn binance_demo_account(base_url: String) -> AccountConfig {
    AccountConfig {
        id: "binance-demo".to_owned(),
        kind: "binance".to_owned(),
        mode: ExecutionMode::Paper,
        api_key_env: Some("PATH".to_owned()),
        api_secret_env: Some("PATH".to_owned()),
        passphrase_env: None,
        use_demo_mode: true,
        reconciliation_remote_snapshot: true,
        execution_remote_submission: None,
        reconciliation_base_url: Some(base_url),
        cash_balance_assets: Vec::new(),
        total_budget_usd: 10_000.0,
    }
}
