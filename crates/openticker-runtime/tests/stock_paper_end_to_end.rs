use openticker_config::{
    AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
    GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
    ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig, SignalMode,
    StorageConfig,
};
use openticker_core::{
    ExecutionMode, IndicatorSignalMetadataFilters, MarketType, OhlcvBar, SignalPhase, Timeframe,
};
use openticker_runtime::Runtime;
use openticker_testkit::close_only_bar;
use std::collections::HashSet;
use std::path::PathBuf;
use toml::Table;

#[test]
fn stock_paper_pipeline_journals_signal_to_fill_chain() {
    let config = fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("aapl").unwrap();

    for close in replay_closes() {
        let bar = test_bar(close);
        let _ = runtime
            .process_bar("aapl", &bar, SignalPhase::Confirmed)
            .unwrap();
    }

    let signals = runtime.recent_signals(500).unwrap();
    assert!(!signals.is_empty());

    let intents = runtime.recent_intents(500).unwrap();
    assert!(intents.iter().any(|intent| intent.intent != "no_op"));

    let risks = runtime.recent_risk_decisions(500).unwrap();
    assert!(risks.iter().any(|decision| decision.decision == "allowed"));

    let orders = runtime.recent_orders(500).unwrap();
    assert!(!orders.is_empty());
    assert!(orders.iter().all(|order| order.status == "submitted"));
    assert!(
        orders
            .iter()
            .all(|order| order.client_order_id.starts_with("alpaca-"))
    );

    let fills = runtime.recent_fills(500).unwrap();
    assert_eq!(fills.len(), orders.len());

    let positions = runtime.recent_positions(500).unwrap();
    assert!(
        positions
            .iter()
            .any(|position| position.reason == "order_filled")
    );

    let executable_intents = intents
        .iter()
        .filter(|intent| intent.intent != "no_op")
        .map(|intent| intent.intent.as_str())
        .collect::<HashSet<_>>();
    let allowed_intents = risks
        .iter()
        .filter(|decision| decision.decision == "allowed")
        .map(|decision| decision.intent.as_str())
        .collect::<HashSet<_>>();

    assert!(
        orders
            .iter()
            .all(|order| executable_intents.contains(order.intent.as_str()))
    );
    assert!(
        orders
            .iter()
            .all(|order| allowed_intents.contains(order.intent.as_str()))
    );

    let order_ids = orders
        .iter()
        .map(|order| order.client_order_id.as_str())
        .collect::<HashSet<_>>();
    assert!(
        fills
            .iter()
            .all(|fill| order_ids.contains(fill.client_order_id.as_str()))
    );

    let signal_events = runtime.recent_events_by_scope("signal", 200).unwrap();
    assert!(
        signal_events
            .iter()
            .any(|event| event.kind == "signal.emitted")
    );
    let order_events = runtime.recent_events_by_scope("order", 200).unwrap();
    assert!(
        order_events
            .iter()
            .any(|event| event.kind == "order.submitted")
    );
}

#[test]
fn shared_account_budget_caps_bots_and_account_simultaneously() {
    let config = two_bot_budget_fixture_bundle();
    let mut runtime = Runtime::from_config(&config);
    runtime.start_instance("aapl-budget").unwrap();
    runtime.start_instance("msft-budget").unwrap();

    for close in replay_closes() {
        let bar = test_bar(close);
        let _ = runtime
            .process_bar("aapl-budget", &bar, SignalPhase::Confirmed)
            .unwrap();
        if runtime
            .recent_orders(200)
            .unwrap()
            .iter()
            .any(|order| order.bot_id == "aapl-budget")
        {
            break;
        }
    }

    for close in replay_closes() {
        let bar = test_bar(close);
        let _ = runtime
            .process_bar("msft-budget", &bar, SignalPhase::Confirmed)
            .unwrap();
        if runtime
            .recent_orders(200)
            .unwrap()
            .iter()
            .any(|order| order.bot_id == "msft-budget")
        {
            break;
        }
    }

    let ledger = runtime.ledger_snapshot();
    let account = ledger
        .accounts
        .iter()
        .find(|account| account.id == "alpaca-paper")
        .expect("shared account budget row should exist");
    let aapl = ledger
        .bots
        .iter()
        .find(|bot| bot.id == "aapl-budget")
        .expect("aapl budget row should exist");
    let msft = ledger
        .bots
        .iter()
        .find(|bot| bot.id == "msft-budget")
        .expect("msft budget row should exist");

    assert!((aapl.allocated_usd - 400.0).abs() < 1e-6);
    assert!((msft.allocated_usd - 600.0).abs() < 1e-6);
    assert!(aapl.attributed_open_notional_usd <= aapl.allocated_usd + 1e-6);
    assert!(msft.attributed_open_notional_usd <= msft.allocated_usd + 1e-6);
    assert!(msft.attributed_open_notional_usd > 0.0);
    assert!(account.total_committed_notional_usd <= account.effective_cap_usd + 1e-6);
}

fn fixture_bundle() -> ConfigBundle {
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
            id: "alpaca-paper".to_owned(),
            kind: "alpaca".to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            use_demo_mode: false,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 1_000.0,
        }],
        risk_profiles: vec![RiskProfileConfig {
            id: "equities-default".to_owned(),
            max_daily_loss_pct: 2.0,
            max_open_positions: 2,
            target_order_notional_usd: Some(1_000.0),
            max_order_notional_usd: 1_000.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        }],
        instances: vec![InstanceConfig {
            id: "aapl".to_owned(),
            enabled: true,
            market: MarketType::Equities,
            symbols: vec!["AAPL".to_owned()],
            timeframe: Timeframe::M1,
            account: "alpaca-paper".to_owned(),
            data_connector: "alpaca".to_owned(),
            execution_connector: "alpaca".to_owned(),
            strategy: "single_indicator_signal".to_owned(),
            signal_mode: SignalMode::Intrabar,
            polling_enabled: true,
            polling_interval_ms: 1_000,
            indicators: vec![IndicatorInstanceConfig {
                id: "trend-1".to_owned(),
                indicator_type: "sma_crossover".to_owned(),
                enabled: true,
                role: None,
                signal_policy: None,
                weight: None,
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::new(),
            }],
            execution_constraints: ExecutionConstraintsConfig::default(),
            budget: BudgetConfig { pct: 100.0 },
            risk: InstanceRiskConfig {
                profile: "equities-default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: None,
            allow_live: false,
        }],
    }
}

fn two_bot_budget_fixture_bundle() -> ConfigBundle {
    let mut bundle = fixture_bundle();
    bundle.accounts[0].total_budget_usd = 1_000.0;
    bundle.risk_profiles[0].target_order_notional_usd = Some(500.0);
    bundle.risk_profiles[0].max_order_notional_usd = 500.0;

    "aapl-budget".clone_into(&mut bundle.instances[0].id);
    bundle.instances[0].budget.pct = 40.0;

    let mut second = bundle.instances[0].clone();
    "msft-budget".clone_into(&mut second.id);
    second.symbols = vec!["MSFT".to_owned()];
    second.budget.pct = 60.0;
    bundle.instances.push(second);

    bundle
}

fn replay_closes() -> Vec<f64> {
    let mut closes = Vec::new();
    let mut close = 125.0;
    for _ in 0..20 {
        close -= 1.4;
        closes.push(close);
    }
    for _ in 0..20 {
        close += 2.3;
        closes.push(close);
    }
    for _ in 0..20 {
        close -= 2.6;
        closes.push(close);
    }
    closes
}

fn test_bar(close: f64) -> OhlcvBar {
    close_only_bar("2030-01-01T00:00:00Z", close)
}
