use openticker_core::{ExecutionMode, OhlcvBar, Timeframe};
use openticker_data::NormalizedOrderEvent;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    Alpaca,
    Binance,
}

impl ConnectorKind {
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "alpaca" => Some(Self::Alpaca),
            "binance" => Some(Self::Binance),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alpaca => "alpaca",
            Self::Binance => "binance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorRole {
    Data,
    Execution,
    Dual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorDescriptor {
    pub kind: ConnectorKind,
    pub role: ConnectorRole,
    pub supports_paper: bool,
    pub supports_live: bool,
    pub supports_demo: bool,
    pub resilience: ConnectorResiliencePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorResiliencePolicy {
    pub reconnect_base_backoff_ms: u64,
    pub reconnect_max_backoff_ms: u64,
    pub reconnect_jitter_bps: u16,
    pub requires_ping_pong_keepalive: bool,
    pub max_connection_lifetime_secs: Option<u64>,
    pub max_messages_per_second: Option<u32>,
    pub single_connection_per_api_key: bool,
}

impl ConnectorResiliencePolicy {
    #[must_use]
    pub fn reconnect_delay_ms(self, consecutive_failures: u32) -> u64 {
        let capped_failures = consecutive_failures.clamp(1, 16);
        let exponent = capped_failures.saturating_sub(1);
        let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let raw_delay = self
            .reconnect_base_backoff_ms
            .saturating_mul(multiplier)
            .min(self.reconnect_max_backoff_ms);
        let jitter = raw_delay.saturating_mul(u64::from(self.reconnect_jitter_bps)) / 10_000;

        raw_delay
            .saturating_add(jitter)
            .min(self.reconnect_max_backoff_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConnectorResilienceState {
    pub consecutive_failures: u32,
    pub last_disconnect_at_ms: Option<i64>,
    pub next_reconnect_at_ms: Option<i64>,
    pub throttled_until_ms: Option<i64>,
}

impl ConnectorResilienceState {
    pub fn note_disconnect(&mut self, now_ms: i64, policy: ConnectorResiliencePolicy) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_disconnect_at_ms = Some(now_ms);

        let delay_ms = policy.reconnect_delay_ms(self.consecutive_failures);
        let delay_ms_i64 = i64::try_from(delay_ms).unwrap_or(i64::MAX);
        self.next_reconnect_at_ms = Some(now_ms.saturating_add(delay_ms_i64));
    }

    pub fn note_reconnect_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_disconnect_at_ms = None;
        self.next_reconnect_at_ms = None;
        self.throttled_until_ms = None;
    }

    pub fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64) {
        let throttle_window_i64 = i64::try_from(throttle_window_ms).unwrap_or(i64::MAX);
        self.throttled_until_ms = Some(now_ms.saturating_add(throttle_window_i64));
    }

    #[must_use]
    pub fn can_attempt_reconnect(self, now_ms: i64) -> bool {
        if let Some(until_ms) = self.throttled_until_ms
            && now_ms < until_ms
        {
            return false;
        }
        if let Some(next_ms) = self.next_reconnect_at_ms {
            return now_ms >= next_ms;
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorStatus {
    pub kind: ConnectorKind,
    pub state: ConnectionState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorOpenOrder {
    pub client_order_id: String,
    pub symbol: String,
    pub status: String,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorPosition {
    pub symbol: String,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConnectorAccountSnapshot {
    pub open_orders: Vec<ConnectorOpenOrder>,
    pub positions: Vec<ConnectorPosition>,
    #[serde(default)]
    pub balances: Vec<ConnectorPrivateBalance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConnectorSymbolConstraints {
    pub fractional_entry_supported: Option<bool>,
    pub quantity_step: Option<f64>,
    pub min_quantity: Option<f64>,
    pub min_notional_usd: Option<f64>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConfirmedBarPage {
    pub bars: Vec<OhlcvBar>,
    pub exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorAccount {
    pub account_id: String,
    pub kind: ConnectorKind,
    pub mode: ExecutionMode,
    pub use_demo_mode: bool,
    pub api_key_env: Option<String>,
    pub api_secret_env: Option<String>,
    pub passphrase_env: Option<String>,
    pub reconciliation_remote_snapshot: bool,
    pub execution_remote_submission: bool,
    pub reconciliation_base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorAccountStatus {
    pub account_id: String,
    pub kind: ConnectorKind,
    pub mode: ExecutionMode,
    pub state: ConnectionState,
    pub message: String,
    pub resilience_policy: ConnectorResiliencePolicy,
    pub resilience_state: ConnectorResilienceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStreamConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectorMarketStreamSubscription {
    pub symbol: String,
    pub timeframe: Timeframe,
}

impl PartialOrd for ConnectorMarketStreamSubscription {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConnectorMarketStreamSubscription {
    fn cmp(&self, other: &Self) -> Ordering {
        self.symbol
            .cmp(&other.symbol)
            .then_with(|| self.timeframe.to_string().cmp(&other.timeframe.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorPreviewStreamEvent {
    ConnectionState {
        state: PreviewStreamConnectionState,
        detail: Option<String>,
    },
    BarUpdate {
        subscription: ConnectorMarketStreamSubscription,
        update: openticker_data::NormalizedBarUpdate,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorPreviewStreamCommand {
    ReplaceSubscriptions(Vec<ConnectorMarketStreamSubscription>),
    Shutdown,
}

pub struct ConnectorPreviewStreamSession {
    command_tx: UnboundedSender<ConnectorPreviewStreamCommand>,
    event_rx: UnboundedReceiver<ConnectorPreviewStreamEvent>,
}

impl ConnectorPreviewStreamSession {
    #[must_use]
    pub fn new(
        command_tx: UnboundedSender<ConnectorPreviewStreamCommand>,
        event_rx: UnboundedReceiver<ConnectorPreviewStreamEvent>,
    ) -> Self {
        Self {
            command_tx,
            event_rx,
        }
    }

    pub fn replace_subscriptions(
        &self,
        subscriptions: Vec<ConnectorMarketStreamSubscription>,
    ) -> Result<(), String> {
        self.command_tx
            .send(ConnectorPreviewStreamCommand::ReplaceSubscriptions(
                subscriptions,
            ))
            .map_err(|error| format!("preview stream session closed: {error}"))
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.command_tx
            .send(ConnectorPreviewStreamCommand::Shutdown)
            .map_err(|error| format!("preview stream session closed: {error}"))
    }

    pub fn try_recv(&mut self) -> Result<ConnectorPreviewStreamEvent, TryRecvError> {
        self.event_rx.try_recv()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorPrivateStreamEvent {
    Order(NormalizedOrderEvent),
    Account(ConnectorPrivateAccountEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorPrivateAccountEvent {
    pub balances: Vec<ConnectorPrivateBalance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorPrivateBalance {
    pub asset: String,
    pub free: f64,
    pub locked: f64,
}
