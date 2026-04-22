use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

#[derive(Debug, Parser)]
#[command(name = "openticker-cli")]
#[command(about = "OpenTicker operator command line")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Dashboard {
        #[command(flatten)]
        options: DashboardOptions,
    },
    ValidateConfig {
        #[arg(long, default_value = "config")]
        config_dir: PathBuf,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    Risk {
        #[command(subcommand)]
        command: RiskCommand,
    },
    Instance {
        #[command(subcommand)]
        command: InstanceCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    Print {
        #[arg(long, default_value_t = false)]
        effective: bool,
        #[arg(long, default_value = "config")]
        config_dir: PathBuf,
    },
    Reload {
        #[command(flatten)]
        api: ApiOptions,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ServiceCommand {
    Run {
        #[arg(long, default_value = "config")]
        config_dir: PathBuf,
        #[arg(long)]
        bind: Option<String>,
    },
    Status {
        #[command(flatten)]
        api: ApiOptions,
    },
    Ledger {
        #[command(flatten)]
        api: ApiOptions,
    },
    Connectors {
        #[command(flatten)]
        api: ApiOptions,
    },
    ConnectorsMatrix {
        #[command(flatten)]
        api: ApiOptions,
    },
    Events {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Signals {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Intents {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    RiskDecisions {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Orders {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Fills {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Positions {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Reconciliations {
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        instance_id: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RiskCommand {
    KillSwitch {
        #[command(subcommand)]
        command: KillSwitchCommand,
    },
    Status {
        #[command(flatten)]
        api: ApiOptions,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum KillSwitchCommand {
    On {
        #[command(flatten)]
        api: ApiOptions,
    },
    Off {
        #[command(flatten)]
        api: ApiOptions,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum InstanceCommand {
    List {
        #[command(flatten)]
        api: ApiOptions,
    },
    Get {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Start {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Stop {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Pause {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Resume {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Reconcile {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    ReconcileReport {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    Tick {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    AutoTick {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
        #[arg(long, default_value_t = 1_000)]
        interval_ms: u64,
        #[arg(long)]
        max_ticks: Option<u64>,
    },
    CancelOpenOrders {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
    ClosePositions {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
    },
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ApiOptions {
    #[arg(long, default_value = DEFAULT_API_URL)]
    pub(crate) api_url: String,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DashboardOptions {
    #[command(flatten)]
    pub(crate) api: ApiOptions,
    #[arg(long, default_value_t = 1_000)]
    pub(crate) refresh_ms: u64,
    #[arg(long, default_value_t = 25)]
    pub(crate) limit: usize,
}
