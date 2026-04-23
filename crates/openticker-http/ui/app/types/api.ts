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
  staleness_ms?: number | null
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
