import type { AccountConfigUpdate, BotConfig, GlobalConfig, RiskOverrides, RiskProfileConfig } from '~/types/api'

/**
 * Client-side, structural-only config validation. These mirror the cheap
 * constraints the backend also enforces so the UI can surface errors inline
 * before a round-trip. Semantic checks (account exists, connector capability,
 * change-set / restart-scoped conflicts) stay server-side and arrive as
 * ConfigViolation entries.
 *
 * Each validator returns a map of dotted field path -> message. Paths are
 * chosen to match the form's field bindings (e.g. `data_plane.default_retention`,
 * `indicators[0].params.fast_length`, `risk.overrides.max_spread_bps`). Backend
 * coarse scopes (`global`, `bot:<id>`, `account:<id>`) map to a banner instead.
 */
export type FieldErrors = Record<string, string>

/** Bot id constraint: <= 64 chars, alphanumeric plus `-` and `_`. */
const BOT_ID_RE = /^[A-Za-z0-9_-]+$/
const BOT_ID_MAX = 64

function isNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

/** Records `message` at `path` only when one is not already present. */
function add(errors: FieldErrors, path: string, message: string): void {
  if (!(path in errors)) errors[path] = message
}

/** Requires a finite number strictly greater than zero. */
export function positive(errors: FieldErrors, path: string, value: unknown, label = 'Value'): void {
  if (!isNumber(value) || value <= 0) add(errors, path, `${label} must be greater than 0`)
}

/** Requires a finite, non-negative integer. */
export function nonNegativeInt(errors: FieldErrors, path: string, value: unknown, label = 'Value'): void {
  if (!isNumber(value) || value < 0 || !Number.isInteger(value)) {
    add(errors, path, `${label} must be a non-negative whole number`)
  }
}

/** Requires a finite integer strictly greater than zero. */
export function positiveInt(errors: FieldErrors, path: string, value: unknown, label = 'Value'): void {
  if (!isNumber(value) || value <= 0 || !Number.isInteger(value)) {
    add(errors, path, `${label} must be a whole number greater than 0`)
  }
}

/**
 * Requires a percentage in the half-open range (0, 100]. Used for `budget.pct`,
 * which the backend rejects at `<= 0` (see `validate_instance_budget`).
 */
export function pct(errors: FieldErrors, path: string, value: unknown, label = 'Percentage'): void {
  if (!isNumber(value) || value <= 0 || value > 100) add(errors, path, `${label} must be between 0 and 100`)
}

/**
 * Requires a percentage in the closed range [0, 100]. Used for
 * `max_daily_loss_pct`: the backend imposes NO lower bound on this field (it is
 * not checked in validation.rs), so a client rejecting 0 would be stricter than
 * the server and block a valid save. Zero is accepted here; the (0, 100] upper
 * bound is retained as a UI sanity guard the server does not contradict.
 */
export function nonNegativePct(errors: FieldErrors, path: string, value: unknown, label = 'Percentage'): void {
  if (!isNumber(value) || value < 0 || value > 100) add(errors, path, `${label} must be between 0 and 100`)
}

function requireNonEmpty(errors: FieldErrors, path: string, value: unknown, label: string): void {
  if (typeof value !== 'string' || value.trim().length === 0) add(errors, path, `${label} is required`)
}

export function validateGlobal(draft: GlobalConfig): FieldErrors {
  const errors: FieldErrors = {}
  requireNonEmpty(errors, 'service.environment', draft.service?.environment, 'Environment')
  requireNonEmpty(errors, 'service.data_dir', draft.service?.data_dir, 'Data directory')
  requireNonEmpty(errors, 'service.bot_dir', draft.service?.bot_dir, 'Bot directory')
  requireNonEmpty(errors, 'http.bind', draft.http?.bind, 'Bind address')
  requireNonEmpty(errors, 'http.openapi_path', draft.http?.openapi_path, 'OpenAPI path')
  requireNonEmpty(errors, 'storage.kind', draft.storage?.kind, 'Storage kind')
  requireNonEmpty(errors, 'storage.path', draft.storage?.path, 'Storage path')
  positiveInt(errors, 'storage.busy_timeout_ms', draft.storage?.busy_timeout_ms, 'Busy timeout')
  requireNonEmpty(errors, 'observability.log_level', draft.observability?.log_level, 'Log level')
  requireNonEmpty(errors, 'observability.metrics_path', draft.observability?.metrics_path, 'Metrics path')

  const dataPlane = draft.data_plane
  if (dataPlane) {
    positiveInt(
      errors,
      'data_plane.default_polling_interval_ms',
      dataPlane.default_polling_interval_ms,
      'Polling interval'
    )
    positiveInt(errors, 'data_plane.default_retention', dataPlane.default_retention, 'Retention')
    dataPlane.watchlist?.forEach((entry, index) => {
      const base = `data_plane.watchlist[${index}]`
      requireNonEmpty(errors, `${base}.account`, entry.account, 'Account')
      requireNonEmpty(errors, `${base}.symbol`, entry.symbol, 'Symbol')
      requireNonEmpty(errors, `${base}.timeframe`, entry.timeframe, 'Timeframe')
      if (entry.polling_interval_ms !== null && entry.polling_interval_ms !== undefined) {
        positiveInt(errors, `${base}.polling_interval_ms`, entry.polling_interval_ms, 'Polling interval')
      }
      if (entry.retention !== null && entry.retention !== undefined) {
        positiveInt(errors, `${base}.retention`, entry.retention, 'Retention')
      }
    })
  }
  return errors
}

export function validateAccount(draft: AccountConfigUpdate): FieldErrors {
  const errors: FieldErrors = {}
  requireNonEmpty(errors, 'kind', draft.kind, 'Kind')
  requireNonEmpty(errors, 'mode', draft.mode, 'Mode')
  if (!isNumber(draft.total_budget_usd) || draft.total_budget_usd < 0) {
    add(errors, 'total_budget_usd', 'Total budget must be a non-negative number')
  }
  return errors
}

/** Validates the risk fields shared by a profile and an override map. */
function validateRiskFields(errors: FieldErrors, prefix: string, value: RiskProfileConfig | RiskOverrides): void {
  const at = (field: string) => (prefix ? `${prefix}.${field}` : field)
  const v = value as Record<string, unknown>

  if (v.max_daily_loss_pct !== null && v.max_daily_loss_pct !== undefined) {
    // Backend imposes no lower bound on max_daily_loss_pct, so allow 0.
    nonNegativePct(errors, at('max_daily_loss_pct'), v.max_daily_loss_pct, 'Max daily loss')
  }
  if (v.max_open_positions !== null && v.max_open_positions !== undefined) {
    nonNegativeInt(errors, at('max_open_positions'), v.max_open_positions, 'Max open positions')
  }
  if (v.target_order_notional_usd !== null && v.target_order_notional_usd !== undefined) {
    positive(errors, at('target_order_notional_usd'), v.target_order_notional_usd, 'Target order notional')
  }
  if (v.max_order_notional_usd !== null && v.max_order_notional_usd !== undefined) {
    positive(errors, at('max_order_notional_usd'), v.max_order_notional_usd, 'Max order notional')
  }
  if (v.max_spread_bps !== null && v.max_spread_bps !== undefined) {
    nonNegativeInt(errors, at('max_spread_bps'), v.max_spread_bps, 'Max spread')
  }
  if (v.max_slippage_bps !== null && v.max_slippage_bps !== undefined) {
    nonNegativeInt(errors, at('max_slippage_bps'), v.max_slippage_bps, 'Max slippage')
  }
  if (v.stale_data_ms !== null && v.stale_data_ms !== undefined) {
    positiveInt(errors, at('stale_data_ms'), v.stale_data_ms, 'Stale data window')
  }
  if (v.cooldown_after_reject_ms !== null && v.cooldown_after_reject_ms !== undefined) {
    nonNegativeInt(errors, at('cooldown_after_reject_ms'), v.cooldown_after_reject_ms, 'Cooldown')
  }

  // When both bounds are present, the cap must not be below the target.
  const target = v.target_order_notional_usd
  const cap = v.max_order_notional_usd
  if (isNumber(target) && isNumber(cap) && cap < target) {
    add(errors, at('max_order_notional_usd'), 'Max order notional must be at least the target order notional')
  }
}

export function validateRiskProfile(draft: RiskProfileConfig): FieldErrors {
  const errors: FieldErrors = {}
  requireNonEmpty(errors, 'id', draft.id, 'Profile id')
  // A profile requires both loss and cap unconditionally (not optional like
  // overrides). The backend does not bound max_daily_loss_pct, so allow 0.
  nonNegativePct(errors, 'max_daily_loss_pct', draft.max_daily_loss_pct, 'Max daily loss')
  positive(errors, 'max_order_notional_usd', draft.max_order_notional_usd, 'Max order notional')
  validateRiskFields(errors, '', draft)
  return errors
}

export function validateBot(draft: BotConfig, options: { create?: boolean } = {}): FieldErrors {
  const errors: FieldErrors = {}

  requireNonEmpty(errors, 'id', draft.id, 'Bot id')
  if (typeof draft.id === 'string' && draft.id.length > 0) {
    if (draft.id.length > BOT_ID_MAX) add(errors, 'id', `Bot id must be at most ${BOT_ID_MAX} characters`)
    // The id becomes a filename on create, so it must be a safe slug.
    if (options.create && !BOT_ID_RE.test(draft.id)) {
      add(errors, 'id', 'Bot id may only contain letters, numbers, hyphens and underscores')
    }
  }

  requireNonEmpty(errors, 'market', draft.market, 'Market')
  requireNonEmpty(errors, 'timeframe', draft.timeframe, 'Timeframe')
  requireNonEmpty(errors, 'account', draft.account, 'Account')
  requireNonEmpty(errors, 'data_connector', draft.data_connector, 'Data connector')
  requireNonEmpty(errors, 'execution_connector', draft.execution_connector, 'Execution connector')
  requireNonEmpty(errors, 'strategy', draft.strategy, 'Strategy')
  requireNonEmpty(errors, 'signal_mode', draft.signal_mode, 'Signal mode')

  if (!Array.isArray(draft.symbols) || draft.symbols.length === 0) {
    add(errors, 'symbols', 'At least one symbol is required')
  } else {
    draft.symbols.forEach((symbol, index) => {
      requireNonEmpty(errors, `symbols[${index}]`, symbol, 'Symbol')
    })
  }

  positiveInt(errors, 'polling_interval_ms', draft.polling_interval_ms, 'Polling interval')

  if (draft.warmup_target_bars !== null && draft.warmup_target_bars !== undefined) {
    positiveInt(errors, 'warmup_target_bars', draft.warmup_target_bars, 'Warmup target bars')
  }

  if (draft.budget) {
    pct(errors, 'budget.pct', draft.budget.pct, 'Budget percentage')
  }

  const ec = draft.execution_constraints
  if (ec) {
    if (ec.quantity_step !== null && ec.quantity_step !== undefined) {
      positive(errors, 'execution_constraints.quantity_step', ec.quantity_step, 'Quantity step')
    }
    if (ec.min_quantity !== null && ec.min_quantity !== undefined) {
      positive(errors, 'execution_constraints.min_quantity', ec.min_quantity, 'Min quantity')
    }
    if (ec.min_notional_usd !== null && ec.min_notional_usd !== undefined) {
      positive(errors, 'execution_constraints.min_notional_usd', ec.min_notional_usd, 'Min notional')
    }
  }

  if (draft.risk) {
    requireNonEmpty(errors, 'risk.profile', draft.risk.profile, 'Risk profile')
    if (draft.risk.overrides) validateRiskFields(errors, 'risk.overrides', draft.risk.overrides)
  }

  const seenIds = new Set<string>()
  draft.indicators?.forEach((indicator, index) => {
    const base = `indicators[${index}]`
    requireNonEmpty(errors, `${base}.id`, indicator.id, 'Indicator id')
    requireNonEmpty(errors, `${base}.type`, indicator.type, 'Indicator type')
    if (typeof indicator.id === 'string' && indicator.id.trim().length > 0) {
      const key = indicator.id.trim()
      if (seenIds.has(key)) add(errors, `${base}.id`, 'Indicator id must be unique')
      else seenIds.add(key)
    }
    if (indicator.weight !== null && indicator.weight !== undefined && !isNumber(indicator.weight)) {
      add(errors, `${base}.weight`, 'Weight must be a number')
    }
  })

  return errors
}
