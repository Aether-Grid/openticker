use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{
    ExecutionMode, IndicatorSignalMetadataFilters, IndicatorSignalPolicy, MarketType, SignalPhase,
    Timeframe,
};
use openticker_runtime::{CycleOutcome, Runtime, ServiceError};
use std::path::PathBuf;
use toml::Table;

#[test]
fn processes_binance_kline_stream_payloads_for_crypto_instances() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("btcusdt").unwrap();

    let preview_payload = binance_kline_payload("BTCUSDT", 1_893_456_000_000, 42_000.0, false);
    let preview = runtime
        .process_market_stream_payload("btcusdt", &preview_payload)
        .unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].phase, SignalPhase::Preview);

    let confirmed_payload = binance_kline_payload("BTCUSDT", 1_893_456_060_000, 42_120.0, true);
    let confirmed = runtime
        .process_market_stream_payload("btcusdt", &confirmed_payload)
        .unwrap();
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].phase, SignalPhase::Confirmed);

    let intents = runtime.recent_intents(20).unwrap();
    assert!(intents.len() >= 2);
}

#[test]
fn recovers_after_malformed_crypto_market_stream_payload() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("btcusdt").unwrap();

    let preview_payload = binance_kline_payload("BTCUSDT", 1_893_456_000_000, 42_000.0, false);
    let preview = runtime
        .process_market_stream_payload("btcusdt", &preview_payload)
        .unwrap();
    assert_eq!(preview.len(), 1);

    let result = runtime.process_market_stream_payload("btcusdt", "not-json");
    assert!(matches!(
        result,
        Err(ServiceError::DataConnectorUnavailable { reason, .. })
            if reason.contains("decode market stream payload")
    ));

    let recovery_payload = binance_kline_payload("BTCUSDT", 1_893_456_060_000, 42_120.0, true);
    let recovered = runtime
        .process_market_stream_payload("btcusdt", &recovery_payload)
        .unwrap();
    assert_eq!(recovered.len(), 1);
}

#[test]
fn dedupes_duplicate_crypto_market_stream_payloads() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("btcusdt").unwrap();

    let duplicate_payload = binance_kline_payload("BTCUSDT", 1_893_456_000_000, 42_000.0, false);
    let first = runtime
        .process_market_stream_payload("btcusdt", &duplicate_payload)
        .unwrap();
    assert_eq!(first.len(), 1);

    let duplicate = runtime
        .process_market_stream_payload("btcusdt", &duplicate_payload)
        .unwrap();
    assert!(duplicate.is_empty());

    let updated_payload = binance_kline_payload("BTCUSDT", 1_893_456_000_000, 42_010.0, false);
    let updated = runtime
        .process_market_stream_payload("btcusdt", &updated_payload)
        .unwrap();
    assert_eq!(updated.len(), 1);
}

#[test]
fn intrabar_preview_updates_submit_orders_when_preview_is_allowed() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("btcusdt").unwrap();

    let warmup_end_index = warmup_with_confirmed_flat_bars(&mut runtime, 0, 60, 42_000.0);
    assert!(runtime.get_instance("btcusdt").unwrap().warmup.ready);

    let orders_before = runtime.recent_orders(200).unwrap().len();
    let fills_before = runtime.recent_fills(200).unwrap().len();
    for (offset, close) in replay_closes().into_iter().enumerate() {
        let payload = binance_kline_payload(
            "BTCUSDT",
            stream_open_time_ms(warmup_end_index + offset),
            close,
            false,
        );
        let outcomes = runtime
            .process_market_stream_payload("btcusdt", &payload)
            .unwrap();
        assert!(!outcomes.is_empty());
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.phase == SignalPhase::Preview)
        );

        if runtime.recent_orders(200).unwrap().len() > orders_before {
            break;
        }
    }

    let orders = runtime.recent_orders(200).unwrap();
    let fills = runtime.recent_fills(200).unwrap();
    assert!(orders.len() > orders_before);
    assert!(fills.len() > fills_before);

    let preview_cycles = runtime
        .recent_cycle_traces_for_bot("btcusdt", Some("BTCUSDT"), Some("preview"), None, None, 200)
        .unwrap();
    assert!(
        preview_cycles
            .iter()
            .any(|cycle| cycle.outcome == CycleOutcome::AcceptedFilled)
    );
}

#[test]
fn confirmed_required_blocks_preview_submissions_but_allows_confirmed_flow() {
    let config = fixture_bundle_with_signal_policy(Some(IndicatorSignalPolicy::ConfirmedRequired));
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("btcusdt").unwrap();

    let warmup_end_index = warmup_with_confirmed_flat_bars(&mut runtime, 0, 60, 42_000.0);
    assert!(runtime.get_instance("btcusdt").unwrap().warmup.ready);

    let orders_before = runtime.recent_orders(200).unwrap().len();
    let fills_before = runtime.recent_fills(200).unwrap().len();
    let closes = replay_closes();

    for (offset, close) in closes.iter().copied().enumerate() {
        let payload = binance_kline_payload(
            "BTCUSDT",
            stream_open_time_ms(warmup_end_index + offset),
            close,
            false,
        );
        let outcomes = runtime
            .process_market_stream_payload("btcusdt", &payload)
            .unwrap();
        assert!(!outcomes.is_empty());
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.phase == SignalPhase::Preview)
        );
    }

    assert_eq!(runtime.recent_orders(200).unwrap().len(), orders_before);
    assert_eq!(runtime.recent_fills(200).unwrap().len(), fills_before);

    let preview_cycles = runtime
        .recent_cycle_traces_for_bot("btcusdt", Some("BTCUSDT"), Some("preview"), None, None, 200)
        .unwrap();
    assert!(!preview_cycles.is_empty());
    assert!(!preview_cycles.iter().any(|cycle| {
        matches!(
            cycle.outcome,
            CycleOutcome::AcceptedNoFill
                | CycleOutcome::AcceptedPartiallyFilled
                | CycleOutcome::AcceptedFilled
        )
    }));

    let confirmed_start_index = warmup_end_index + closes.len();
    for (offset, close) in closes.into_iter().enumerate() {
        let payload = binance_kline_payload(
            "BTCUSDT",
            stream_open_time_ms(confirmed_start_index + offset),
            close,
            true,
        );
        let outcomes = runtime
            .process_market_stream_payload("btcusdt", &payload)
            .unwrap();
        assert!(!outcomes.is_empty());
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.phase == SignalPhase::Confirmed)
        );

        if runtime.recent_orders(200).unwrap().len() > orders_before {
            break;
        }
    }

    let orders = runtime.recent_orders(200).unwrap();
    let fills = runtime.recent_fills(200).unwrap();
    assert!(orders.len() > orders_before);
    assert!(fills.len() > fills_before);

    let confirmed_cycles = runtime
        .recent_cycle_traces_for_bot(
            "btcusdt",
            Some("BTCUSDT"),
            Some("confirmed"),
            None,
            None,
            200,
        )
        .unwrap();
    assert!(
        confirmed_cycles
            .iter()
            .any(|cycle| cycle.outcome == CycleOutcome::AcceptedFilled)
    );
}

fn replay_closes() -> Vec<f64> {
    let mut closes = Vec::new();
    let mut close = 42_000.0;
    for _ in 0..20 {
        close -= 185.0;
        closes.push(close);
    }
    for _ in 0..20 {
        close += 260.0;
        closes.push(close);
    }
    for _ in 0..20 {
        close -= 315.0;
        closes.push(close);
    }
    closes
}

fn warmup_with_confirmed_flat_bars(
    runtime: &mut Runtime,
    start_index: usize,
    bars: usize,
    close: f64,
) -> usize {
    let mut stream_index = start_index;
    for _ in 0..bars {
        let payload =
            binance_kline_payload("BTCUSDT", stream_open_time_ms(stream_index), close, true);
        let outcomes = runtime
            .process_market_stream_payload("btcusdt", &payload)
            .unwrap();
        assert!(!outcomes.is_empty());
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.phase == SignalPhase::Confirmed)
        );
        stream_index += 1;
    }

    stream_index
}

fn stream_open_time_ms(index: usize) -> i64 {
    let index_i64 = i64::try_from(index).expect("stream index should fit i64");
    1_893_456_000_000 + (index_i64 * 60_000)
}

fn binance_kline_payload(symbol: &str, open_time_ms: i64, close: f64, is_closed: bool) -> String {
    let open = close - 25.0;
    let high = close + 30.0;
    let low = close - 35.0;
    let volume = 12.5;

    format!(
        "{{\"stream\":\"{}@kline_1m\",\"data\":{{\"e\":\"kline\",\"s\":\"{}\",\"k\":{{\"t\":{},\"o\":\"{:.2}\",\"h\":\"{:.2}\",\"l\":\"{:.2}\",\"c\":\"{:.2}\",\"v\":\"{:.2}\",\"x\":{}}}}}}}",
        symbol.to_lowercase(),
        symbol,
        open_time_ms,
        open,
        high,
        low,
        close,
        volume,
        is_closed
    )
}

fn fixture_bundle() -> ConfigBundle {
    fixture_bundle_with_signal_policy(None)
}

fn fixture_bundle_with_signal_policy(signal_policy: Option<IndicatorSignalPolicy>) -> ConfigBundle {
    ConfigBundle {
        global: GlobalConfig {
            service: ServiceConfig {
                environment: "test".to_owned(),
                data_dir: "./var".into(),
                bot_dir: "./config/bots".into(),
            },
            http: HttpConfig {
                enabled: true,
                bind: "127.0.0.1:8080".to_owned(),
                request_log: true,
                openapi_enabled: true,
                openapi_path: "/openapi.json".to_owned(),
            },
            storage: StorageConfig {
                kind: "sqlite".to_owned(),
                path: PathBuf::from("./var/openticker-test.db"),
                busy_timeout_ms: 5_000,
                prune_removed_bots_on_startup: false,
            },
            observability: ObservabilityConfig {
                log_level: "info".to_owned(),
                metrics_enabled: true,
                metrics_path: "/metrics".to_owned(),
            },
            safety: SafetyConfig {
                require_explicit_live_enable: true,
                default_start_paused_if_recovery_uncertain: true,
            },
            data_plane: DataPlaneConfig::default(),
        },
        accounts: vec![AccountConfig {
            id: "binance-demo".to_owned(),
            kind: "binance".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: true,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 1_000.0,
        }],
        risk_profiles: vec![RiskProfileConfig {
            id: "crypto-default".to_owned(),
            max_daily_loss_pct: 2.0,
            max_open_positions: 3,
            target_order_notional_usd: Some(5_000.0),
            max_order_notional_usd: 5_000.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        }],
        instances: vec![InstanceConfig {
            id: "btcusdt".to_owned(),
            enabled: true,
            market: MarketType::Crypto,
            symbols: vec!["BTCUSDT".to_owned()],
            timeframe: Timeframe::M1,
            account: "binance-demo".to_owned(),
            data_connector: "binance".to_owned(),
            execution_connector: "binance".to_owned(),
            strategy: "single_indicator_signal".to_owned(),
            signal_mode: SignalMode::Intrabar,
            polling_enabled: true,
            polling_interval_ms: 1_000,
            indicators: vec![IndicatorInstanceConfig {
                id: "trend-1".to_owned(),
                indicator_type: "sma_crossover".to_owned(),
                enabled: true,
                role: None,
                signal_policy,
                weight: None,
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::new(),
            }],
            execution_constraints: ExecutionConstraintsConfig::default(),
            budget: BudgetConfig { pct: 100.0 },
            risk: InstanceRiskConfig {
                profile: "crypto-default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: None,
            allow_live: false,
        }],
    }
}
