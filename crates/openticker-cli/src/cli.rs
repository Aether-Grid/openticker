use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

/// Minimum permitted auto-tick interval, in milliseconds.
///
/// A value of `0` would turn [`InstanceCommand::AutoTick`] into a busy loop that
/// hammers the API as fast as it can respond, so the parsed value is clamped up
/// to this floor.
pub(crate) const MIN_AUTO_TICK_INTERVAL_MS: u64 = 100;

/// Validates an operator-supplied API base URL at argument-parse time.
///
/// A mistyped or malicious `--api-url` would otherwise be `format!`'d directly
/// into request URLs (see `api::api_request_json`), sending trading commands to
/// the wrong host. We require a syntactically valid absolute URL whose scheme is
/// `http` or `https`; anything else (e.g. `file:`, `ftp:`, a bare host) is
/// rejected here so it never reaches the request layer.
///
/// # Errors
///
/// Returns an error string when the value is not a parseable URL or uses a
/// scheme other than `http`/`https`.
pub(crate) fn parse_api_url(value: &str) -> Result<String, String> {
    let parsed =
        url::Url::parse(value).map_err(|error| format!("invalid api url `{value}`: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(value.to_owned()),
        other => Err(format!(
            "invalid api url `{value}`: scheme `{other}` is not supported (use http or https)"
        )),
    }
}

/// Clamps an auto-tick interval up to [`MIN_AUTO_TICK_INTERVAL_MS`].
///
/// Clap parses the raw `u64` first; this normaliser then raises any value below
/// the floor (including `0`) so callers never busy-loop. Clamping rather than
/// rejecting keeps the command forgiving for operators who pass a small value.
pub(crate) fn parse_auto_tick_interval_ms(value: &str) -> Result<u64, String> {
    let raw: u64 = value
        .parse()
        .map_err(|error| format!("invalid interval_ms `{value}`: {error}"))?;
    Ok(raw.max(MIN_AUTO_TICK_INTERVAL_MS))
}

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
        /// Skip the interactive confirmation prompt for this destructive operation.
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        yes: bool,
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
        #[arg(long, default_value_t = 1_000, value_parser = parse_auto_tick_interval_ms)]
        interval_ms: u64,
        #[arg(long)]
        max_ticks: Option<u64>,
    },
    CancelOpenOrders {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
        /// Skip the interactive confirmation prompt for this destructive operation.
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        yes: bool,
    },
    ClosePositions {
        id: String,
        #[command(flatten)]
        api: ApiOptions,
        /// Skip the interactive confirmation prompt for this destructive operation.
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ApiOptions {
    #[arg(long, default_value = DEFAULT_API_URL, value_parser = parse_api_url)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_api_url_accepts_http_and_https() {
        assert_eq!(
            parse_api_url("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            parse_api_url("https://ops.example.com").unwrap(),
            "https://ops.example.com"
        );
    }

    #[test]
    fn parse_api_url_rejects_non_http_schemes() {
        for value in ["file:///etc/passwd", "ftp://host/x", "ws://host/x"] {
            let error = parse_api_url(value).expect_err("non-http(s) scheme must be rejected");
            assert!(
                error.contains("not supported"),
                "unexpected error for {value}: {error}"
            );
        }
    }

    #[test]
    fn parse_api_url_rejects_unparseable_values() {
        let error = parse_api_url("not a url").expect_err("garbage must be rejected");
        assert!(error.contains("invalid api url"), "got: {error}");
    }

    #[test]
    fn parse_auto_tick_interval_clamps_below_minimum() {
        assert_eq!(
            parse_auto_tick_interval_ms("0").unwrap(),
            MIN_AUTO_TICK_INTERVAL_MS
        );
        assert_eq!(
            parse_auto_tick_interval_ms("1").unwrap(),
            MIN_AUTO_TICK_INTERVAL_MS
        );
        assert_eq!(
            parse_auto_tick_interval_ms("99").unwrap(),
            MIN_AUTO_TICK_INTERVAL_MS
        );
    }

    #[test]
    fn parse_auto_tick_interval_preserves_values_at_or_above_minimum() {
        assert_eq!(
            parse_auto_tick_interval_ms("100").unwrap(),
            MIN_AUTO_TICK_INTERVAL_MS
        );
        assert_eq!(parse_auto_tick_interval_ms("1000").unwrap(), 1_000);
    }

    #[test]
    fn parse_auto_tick_interval_rejects_non_numeric() {
        assert!(parse_auto_tick_interval_ms("fast").is_err());
    }
}
