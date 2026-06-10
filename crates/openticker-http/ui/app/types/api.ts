export type IsoTimestamp = string

export interface ServiceStatus {
  total_instances?: number
  running_instances?: number
  paused_instances?: number
  stopped_instances?: number
  reconciling_instances?: number
  reconciliation_blocked_instances?: number
  warmup_ready_instances?: number
  warmup_pending_instances?: number
  warmup_failed_instances?: number
  kill_switch_active?: boolean
  ready?: boolean
  live_mode_active?: boolean
  mode_banner?: string
  connector_resilience_windows_active?: number
  observability?: Record<string, unknown>
  connector_statuses?: unknown[]
  [key: string]: unknown
}

export interface BotPollingStatus {
  last_poll_at?: IsoTimestamp | null
  last_error?: string | null
  last_bar_timestamp?: IsoTimestamp | null
}

export interface BotSummary {
  id: string
  display_name?: string
  account?: string
  venue?: string
  market?: string
  mode?: string
  state?: string
  enabled?: boolean
  symbols?: string[]
  tickers?: string[]
  connector?: string
  readiness?: string
  warmup?: string
  position_quantity?: number
  position_value?: number
  pnl_total?: number
  pnl_realized?: number
  pnl_unrealized?: number
  polling?: BotPollingStatus
  reconciliation?: {
    status?: string
    blocked?: boolean
    last_report_at?: IsoTimestamp | null
  }
  [key: string]: unknown
}

export interface OhlcvBar {
  open: number
  high: number
  low: number
  close: number
  volume: number
  timestamp: IsoTimestamp
}

export interface StreamKey {
  account_id: string
  symbol: string
  timeframe: string
}

export interface StreamStatus {
  key: StreamKey
  retention?: number
  polling_interval_ms?: number
  close_poll_retry_ms?: number | null
  close_poll_grace_ms?: number | null
  last_attempt_ms?: number | null
  last_success_ms?: number | null
  last_error?: string | null
  latest_bar?: OhlcvBar | null
  fetch_count?: number
  error_count?: number
  transport_staleness_ms?: number | null
  staleness_ms?: number | null
  confirmed_bar_close_ms?: number | null
  confirmed_bar_staleness_ms?: number | null
  confirmed_bar_stale_deadline_ms?: number | null
  latest_preview_bar?: OhlcvBar | null
  preview_enabled?: boolean
  preview_connection_state?: string | null
  last_preview_update_ms?: number | null
  last_preview_error?: string | null
  last_confirmed_update_source?: string | null
  attached_instances?: string[]
  sparkline?: number[]
  [key: string]: unknown
}

export interface ConnectorStatus {
  account: string
  kind?: string
  mode?: string
  state?: string
  resilience?: string | Record<string, unknown>
  message?: string
  [key: string]: unknown
}

export interface ConnectorMatrixEntry {
  kind: string
  capabilities?: string[]
  transports?: string[]
  features?: Record<string, boolean>
  [key: string]: unknown
}

export interface LedgerAccount {
  id: string
  kind?: string
  mode?: string
  declared_cap_usd?: number
  live_balance_usd?: number | null
  effective_cap_usd?: number
  attributed_open_notional_usd?: number
  reserved_open_notional_usd?: number
  unattributed_open_notional_usd?: number
  total_committed_notional_usd?: number
  blocked_open_room_usd?: number
  tradeable_open_room_usd?: number
  [key: string]: unknown
}

export interface LedgerBot {
  id: string
  account?: string
  allocation_percent?: number
  committed_notional_usd?: number
  open_notional_usd?: number
  reserved_notional_usd?: number
  [key: string]: unknown
}

export interface LedgerLane {
  account: string
  bot: string
  symbol: string
  open_notional_usd?: number
  position_quantity?: number
  entry_price?: number
  [key: string]: unknown
}

export interface LedgerPortfolio {
  accounts: LedgerAccount[]
  bots: LedgerBot[]
  lanes: LedgerLane[]
  [key: string]: unknown
}

export interface ActivityRecord {
  id?: string | number
  kind?: string
  bot_id?: string
  symbol?: string
  reason?: string
  timestamp?: IsoTimestamp
  payload?: unknown
  origin?: string
  created_at_ms?: number
  [key: string]: unknown
}

export interface RuntimeEvent {
  id: number
  scope: string
  entity_id?: string
  trace_id?: string
  kind: string
  payload: string
  created_at_ms: number
}

export interface SignalRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp: string
  phase: string
  signal: string
  close: number
  metadata_json?: string
  created_at_ms: number
}

export interface IntentRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp: string
  signal: string
  intent: string
  metadata_json?: string
  strategy_rationale?: string
  has_position_before: boolean
  created_at_ms: number
}

export interface RiskDecisionRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp: string
  intent: string
  decision: string
  reason?: string
  created_at_ms: number
}

export interface OrderRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp?: string
  client_order_id: string
  intent: string
  status: string
  price: number
  quantity: number
  created_at_ms: number
}

export interface FillRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp?: string
  client_order_id: string
  price: number
  quantity: number
  fee_asset?: string
  fee_amount?: number
  fee_normalized_usd?: number
  created_at_ms: number
}

export interface PositionRecord {
  id: number
  bot_id: string
  symbol?: string
  trace_id?: string
  bar_timestamp?: string
  has_position: boolean
  quantity: number
  entry_price?: number
  realized_pnl_usd: number
  reason: string
  created_at_ms: number
}

export interface ReconciliationRecord {
  id: number
  bot_id: string
  source: string
  symbol: string
  safe_to_trade: boolean
  local_open_orders: number
  connector_open_orders: number
  local_has_position: boolean
  connector_has_position: boolean
  reason: string
  created_at_ms: number
}

export type CycleOutcome =
  | 'no_op'
  | 'risk_rejected'
  | 'accepted_no_fill'
  | 'accepted_partially_filled'
  | 'accepted_filled'
  | 'failed'

export type CycleRiskDecision = 'allowed' | 'rejected'

export type CyclePhase = 'preview' | 'confirmed'

export type IndicatorSignalKind = 'none' | 'buy_preview' | 'buy_confirmed' | 'sell_preview' | 'sell_confirmed'

export type TradeIntent = 'no_op' | 'open_long' | 'add_long' | 'reduce_long' | 'close_long'

export interface CycleSummary {
  trace_id: string
  bot_id: string
  symbol: string
  bar_timestamp: string
  phase: CyclePhase
  trigger_kind: string
  signal: IndicatorSignalKind | string
  intent: TradeIntent | string
  risk_decision: CycleRiskDecision
  outcome: CycleOutcome
  created_at_ms: number
}

export interface CycleTrigger {
  trigger_kind: string
  phase: CyclePhase
  bar_timestamp: string
  close: number
  signal_source: string
}

export interface SignalStepPayload {
  signal: IndicatorSignalKind | string
  close: number
  metadata?: Record<string, unknown>
}

export interface IntentStepPayload {
  signal: IndicatorSignalKind | string
  intent: TradeIntent | string
  strategy_rationale?: string
  has_position_before: boolean
  order_quantity: number
  order_quantity_adjustment_reason?: string
  order_ledger_outcome?: string
}

export interface RiskStepPayload {
  intent: TradeIntent | string
  decision: CycleRiskDecision
  reason?: string
  stale_data: boolean
  stale_data_diagnostics?: {
    bar_timestamp_ms: number
    close_timestamp_ms: number
    stale_deadline_ms: number
    evaluated_at_ms: number
  }
  cooldown_active: boolean
  account_open_positions: number
  account_daily_loss_pct: number
  observed_spread_bps: number
  estimated_slippage_bps: number
  budget_room: {
    remaining_bot_usd: number
    remaining_account_usd: number
  }
}

export interface ExecutionOrderPayload {
  client_order_id: string
  intent: TradeIntent | string
  status: string
  price: number
  quantity: number
}

export interface ExecutionFillPayload {
  client_order_id: string
  price: number
  quantity: number
  fee_asset?: string
  fee_amount?: number
  fee_normalized_usd?: number
}

export interface ExecutionStepPayload {
  order?: ExecutionOrderPayload
  fill?: ExecutionFillPayload
}

export interface PositionStepPayload {
  has_position_before: boolean
  has_position_after: boolean
  quantity_after: number
  entry_price_after?: number
  realized_pnl_usd: number
  reason?: string
}

export interface ReconciliationSnapshotPayload {
  source: string
  safe_to_trade: boolean
  local_open_orders: number
  connector_open_orders: number
  local_has_position: boolean
  connector_has_position: boolean
  reason: string
  created_at_ms: number
}

export interface CycleTrace {
  summary: CycleSummary
  trigger: CycleTrigger
  signal_step: SignalStepPayload
  intent_step: IntentStepPayload
  risk_step: RiskStepPayload
  execution_step: ExecutionStepPayload
  position_step: PositionStepPayload
  capital_before?: Record<string, unknown>
  capital_after?: Record<string, unknown>
  reconciliation_context?: { latest?: ReconciliationSnapshotPayload }
  related_records?: Array<Record<string, unknown>>
  related_events?: Array<Record<string, unknown>>
}

export interface DashboardSnapshot {
  status?: ServiceStatus
  instances?: BotSummary[]
  dataStreams?: StreamStatus[]
  signals?: { items?: ActivityRecord[] } | ActivityRecord[]
  intents?: { items?: ActivityRecord[] } | ActivityRecord[]
  risk?: { items?: ActivityRecord[] } | ActivityRecord[]
  orders?: { items?: ActivityRecord[] } | ActivityRecord[]
  fills?: { items?: ActivityRecord[] } | ActivityRecord[]
  positions?: { items?: ActivityRecord[] } | ActivityRecord[]
  reconciliations?: { items?: ActivityRecord[] } | ActivityRecord[]
  events?: { items?: ActivityRecord[] } | ActivityRecord[]
  providerEvents?: { items?: ActivityRecord[] } | ActivityRecord[]
  connectorsStatus?: ConnectorStatus[]
  connectorsMatrix?: ConnectorMatrixEntry[]
  ledger?: LedgerPortfolio
  [key: string]: unknown
}

export interface OpenApiSpec {
  openapi?: string
  info?: { title?: string; version?: string }
  paths?: Record<string, Record<string, { summary?: string; operationId?: string }>>
  [key: string]: unknown
}

// ---------------------------------------------------------------------------
// Config editing
//
// These types mirror the Rust config model exactly. The wire (JSON) field
// names and enum string values come from openticker-config/src/model.rs and
// the core enum definitions (Timeframe, MarketType, ExecutionMode,
// IndicatorRole, IndicatorSignalPolicy). Notable serde renames preserved here:
//   - EffectiveConfig.instances is serialized as `bots`
//   - IndicatorInstanceConfig.indicator_type is serialized as `type`
//   - The accounts list is `risk_profiles` (not `risk`)
// AccountConfigUpdate intentionally omits `id` and the three `*_env` secret
// fields; the read shape (EffectiveAccountConfig) carries `secret_status`.
// ---------------------------------------------------------------------------

export type Timeframe = '1m' | '5m' | '15m' | '30m' | '1h' | '4h' | '1d'

export type MarketType = 'equities' | 'crypto'

export type ExecutionMode = 'paper' | 'live'

export type SignalMode = 'intrabar' | 'confirmed_only'

export type IndicatorRole = 'primary_signal' | 'filter' | 'context' | 'risk_helper' | 'research_only'

export type IndicatorSignalPolicy = 'preview_allowed' | 'confirmed_required'

export type SignalStrength = 'normal' | 'strong'

export interface SignalMetadataFilter {
  allowed_strengths?: SignalStrength[]
  allowed_reason_codes?: string[]
  required_tags_all?: string[]
  required_tags_any?: string[]
  blocked_tags?: string[]
}

export interface IndicatorSignalMetadataFilters {
  entry?: SignalMetadataFilter
  exit?: SignalMetadataFilter
}

export interface DataPlaneWatchlistEntry {
  account: string
  symbol: string
  timeframe: Timeframe
  polling_interval_ms?: number | null
  retention?: number | null
}

export interface DataPlaneConfig {
  default_polling_interval_ms: number
  default_retention: number
  watchlist: DataPlaneWatchlistEntry[]
}

export interface ServiceConfig {
  environment: string
  data_dir: string
  bot_dir: string
}

export interface HttpConfig {
  enabled: boolean
  bind: string
  request_log: boolean
  openapi_enabled: boolean
  openapi_path: string
}

export interface StorageConfig {
  kind: string
  path: string
  busy_timeout_ms: number
  prune_removed_bots_on_startup: boolean
}

export interface ObservabilityConfig {
  log_level: string
  metrics_enabled: boolean
  metrics_path: string
}

export interface SafetyConfig {
  require_explicit_live_enable: boolean
  default_start_paused_if_recovery_uncertain: boolean
}

export interface GlobalConfig {
  service: ServiceConfig
  http: HttpConfig
  storage: StorageConfig
  observability: ObservabilityConfig
  safety: SafetyConfig
  data_plane: DataPlaneConfig
}

export interface AccountSecretStatus {
  api_key_present: boolean
  api_secret_present: boolean
  passphrase_present: boolean
}

/** Read shape: secrets are redacted to a presence-only `secret_status`. */
export interface EffectiveAccountConfig {
  id: string
  kind: string
  mode: ExecutionMode
  use_demo_mode: boolean
  reconciliation_remote_snapshot: boolean
  execution_remote_submission: boolean
  reconciliation_base_url: string | null
  cash_balance_assets: string[]
  total_budget_usd: number
  secret_status: AccountSecretStatus
}

/**
 * Write shape for `PUT /v1/config/accounts/{id}`. Mirrors AccountConfig minus
 * `id` and the three `*_env` secret references. The backend uses
 * `deny_unknown_fields`, so sending `api_key_env` etc. is a 422.
 */
export interface AccountConfigUpdate {
  kind: string
  mode: ExecutionMode
  use_demo_mode: boolean
  reconciliation_remote_snapshot: boolean
  execution_remote_submission?: boolean | null
  reconciliation_base_url?: string | null
  cash_balance_assets: string[]
  total_budget_usd: number
}

export interface RiskProfileConfig {
  id: string
  max_daily_loss_pct: number
  max_open_positions: number
  target_order_notional_usd?: number | null
  max_order_notional_usd: number
  max_spread_bps: number
  max_slippage_bps: number
  stale_data_ms: number
  cooldown_after_reject_ms: number
}

/** Per-instance risk overrides: same fields as RiskProfileConfig minus `id`, all optional. */
export interface RiskOverrides {
  max_daily_loss_pct?: number | null
  max_open_positions?: number | null
  target_order_notional_usd?: number | null
  max_order_notional_usd?: number | null
  max_spread_bps?: number | null
  max_slippage_bps?: number | null
  stale_data_ms?: number | null
  cooldown_after_reject_ms?: number | null
}

export interface InstanceRiskConfig {
  profile: string
  overrides: RiskOverrides
}

export interface ExecutionConstraintsConfig {
  quantity_step?: number | null
  min_quantity?: number | null
  min_notional_usd?: number | null
}

export interface BudgetConfig {
  pct: number
}

export interface IndicatorInstanceConfig {
  id: string
  /** Rust field `indicator_type`, serialized as `type`. */
  type: string
  enabled: boolean
  role?: IndicatorRole | null
  signal_policy?: IndicatorSignalPolicy | null
  weight?: number | null
  metadata_filters?: IndicatorSignalMetadataFilters
  /** Open key/value map (TOML table on the backend). */
  params: Record<string, unknown>
}

/** The InstanceConfig shape; named BotConfig for the UI. */
export interface BotConfig {
  id: string
  enabled: boolean
  market: MarketType
  symbols: string[]
  timeframe: Timeframe
  account: string
  data_connector: string
  execution_connector: string
  strategy: string
  signal_mode: SignalMode
  polling_enabled: boolean
  polling_interval_ms: number
  indicators: IndicatorInstanceConfig[]
  execution_constraints: ExecutionConstraintsConfig
  budget: BudgetConfig
  risk: InstanceRiskConfig
  warmup_target_bars?: number | null
  allow_live: boolean
}

export interface EffectiveConfig {
  global: GlobalConfig
  accounts: EffectiveAccountConfig[]
  risk_profiles: RiskProfileConfig[]
  bots: BotConfig[]
  /** Present on read paths that attach the current reload generation. */
  generation?: number
}

export type ConfigReloadTrigger = 'manual_api' | 'config_write' | 'file_watcher'

export type ConfigReloadOutcome = 'reloaded' | 'no_change' | 'rejected' | 'failed'

export interface ConfigViolation {
  /** Stable machine-readable code, e.g. `storage_changed`. */
  code: string
  /** `"global"`, `"account:<id>"`, or `"bot:<id>"`. */
  scope: string
  message: string
}

export interface ConfigErrorResponse {
  error: string
  violations?: ConfigViolation[]
}

export interface ConfigReloadStatus {
  at_ms: number
  trigger: ConfigReloadTrigger
  outcome: ConfigReloadOutcome
  error: string | null
  violations: ConfigViolation[]
  generation: number
}

export interface ConfigReloadStatusResponse {
  generation: number
  last: ConfigReloadStatus | null
  history: ConfigReloadStatus[]
}

/** Result of a config mutation; 422/409 are returned for inline rendering, not thrown. */
export interface ConfigSaveResult<T = unknown> {
  ok: boolean
  status?: number
  error?: string
  violations?: ConfigViolation[]
  /** Success-body entity (global/bot/risk_profile/account), when present. */
  entity?: T
  /** Reload generation after the write. */
  generation?: number
}
