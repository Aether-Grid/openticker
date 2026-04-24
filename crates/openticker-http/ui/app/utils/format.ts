import { format, formatDistanceToNowStrict, parseISO, isValid } from 'date-fns'

const nf2 = new Intl.NumberFormat('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
const nf4 = new Intl.NumberFormat('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 4 })
const nfCompact = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 2 })
const nfPct = new Intl.NumberFormat('en-US', { style: 'percent', minimumFractionDigits: 1, maximumFractionDigits: 2 })

export function fmtNumber(value: number | null | undefined, digits: 0 | 2 | 4 = 2): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  if (digits === 4) return nf4.format(value)
  if (digits === 0) return new Intl.NumberFormat('en-US').format(value)
  return nf2.format(value)
}

export function fmtCompact(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  return nfCompact.format(value)
}

export function fmtUsd(value: number | null | undefined, compact = false): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  if (compact) {
    const formatter = new Intl.NumberFormat('en-US', {
      notation: 'compact',
      style: 'currency',
      currency: 'USD',
      maximumFractionDigits: 2
    })
    return formatter.format(value)
  }
  return nf2.format(value)
}

export function fmtPercent(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  return nfPct.format(value)
}

export function fmtPnL(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  const sign = value > 0 ? '+' : ''
  return `${sign}${nf2.format(value)}`
}

export function pnlColor(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return 'text-ink-soft'
  if (value > 0) return 'text-[color:var(--color-pastel-green-ink)]'
  if (value < 0) return 'text-[color:var(--color-pastel-red-ink)]'
  return 'text-ink-soft'
}

export function fmtDate(iso: string | null | undefined, pattern = 'MMM d, HH:mm'): string {
  if (!iso) return '—'
  const d = parseISO(iso)
  if (!isValid(d)) return '—'
  return format(d, pattern)
}

export function fmtDateTime(iso: string | null | undefined): string {
  return fmtDate(iso, 'MMM d, yyyy HH:mm:ss')
}

export function fmtRelative(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = parseISO(iso)
  if (!isValid(d)) return '—'
  return `${formatDistanceToNowStrict(d)} ago`
}

export function fmtRelativeMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—'
  const d = new Date(ms)
  if (!isValid(d)) return '—'
  return `${formatDistanceToNowStrict(d)} ago`
}

export function fmtDateTimeMs(ms: number | null | undefined, pattern = 'MMM d, HH:mm:ss'): string {
  if (ms === null || ms === undefined) return '—'
  const d = new Date(ms)
  if (!isValid(d)) return '—'
  return format(d, pattern)
}

export function msToIso(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null
  const d = new Date(ms)
  if (!isValid(d)) return null
  return d.toISOString()
}

export function tryParseJson(value: unknown): unknown {
  if (typeof value !== 'string') return value
  if (!value) return value
  const trimmed = value.trim()
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return value
  try {
    return JSON.parse(trimmed)
  } catch {
    return value
  }
}

export function short(value: string | null | undefined, n = 10): string {
  if (!value) return '—'
  if (value.length <= n) return value
  return `${value.slice(0, n)}…`
}

export function sparkPoints(values: number[], width = 120, height = 32): string {
  if (!values || values.length < 2) return ''
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = max - min || 1
  const stepX = width / (values.length - 1)
  return values
    .map((v, i) => `${(i * stepX).toFixed(2)},${(height - ((v - min) / span) * height).toFixed(2)}`)
    .join(' ')
}

export function extractItems<T>(payload: { items?: T[] } | T[] | undefined | null): T[] {
  if (!payload) return []
  if (Array.isArray(payload)) return payload
  if (Array.isArray(payload.items)) return payload.items
  return []
}

export type Freshness = 'ok' | 'warn' | 'stale' | 'error'
export type PreviewHealth = 'off' | 'live' | 'lagging' | 'stale' | 'error'

export function timeframeToMs(timeframe: string | null | undefined): number | null {
  const match = (timeframe ?? '').match(/^(\d+)(m|h|d)$/i)
  if (!match) return null
  const value = Number(match[1])
  if (!Number.isFinite(value) || value <= 0) return null
  switch (match[2]?.toLowerCase()) {
    case 'm':
      return value * 60_000
    case 'h':
      return value * 60 * 60_000
    case 'd':
      return value * 24 * 60 * 60_000
    default:
      return null
  }
}

export function streamFreshness(
  stalenessMs: number | null | undefined,
  lastError?: string | null,
  pollingIntervalMs?: number | null,
  closePollGraceMs?: number | null
): Freshness {
  if (lastError) return 'error'
  if (stalenessMs === null || stalenessMs === undefined) return 'ok'
  const cadenceMs = Math.max(pollingIntervalMs ?? 60_000, 1)
  const graceMs = Math.max(closePollGraceMs ?? 0, 0)
  const warnAtMs = Math.max(60_000, cadenceMs + graceMs)
  const staleAtMs = Math.max(5 * 60_000, cadenceMs * 2 + graceMs)
  if (stalenessMs < warnAtMs) return 'ok'
  if (stalenessMs < staleAtMs) return 'warn'
  return 'stale'
}

export function closedBarFreshness(
  confirmedBarStalenessMs: number | null | undefined,
  lastError?: string | null,
  pollingIntervalMs?: number | null,
  closePollGraceMs?: number | null,
  fallbackTransportStalenessMs?: number | null,
  timeframe?: string | null
): Freshness {
  if (lastError) return 'error'
  if (confirmedBarStalenessMs !== null && confirmedBarStalenessMs !== undefined) {
    const cadenceMs = Math.max(pollingIntervalMs ?? 60_000, 1)
    const riskGraceMs = Math.max(closePollGraceMs ?? 0, 0)
    const timeframeMs = timeframeToMs(timeframe) ?? 60_000
    const closeSlackMs = Math.max(cadenceMs * 2, riskGraceMs, 60_000)
    const warnAtMs = closeSlackMs
    const staleAtMs = Math.max(timeframeMs + closeSlackMs, closeSlackMs * 2)
    if (confirmedBarStalenessMs <= warnAtMs) return 'ok'
    if (confirmedBarStalenessMs <= staleAtMs) return 'warn'
    return 'stale'
  }
  return streamFreshness(fallbackTransportStalenessMs, lastError, pollingIntervalMs, closePollGraceMs)
}

export function previewHealth(
  enabled: boolean | null | undefined,
  connectionState: string | null | undefined,
  lastPreviewUpdateMs: number | null | undefined,
  lastPreviewError: string | null | undefined,
  timeframe: string | null | undefined,
  nowMs = Date.now()
): PreviewHealth {
  if (!enabled) return 'off'
  if (lastPreviewError) return 'error'
  const state = (connectionState ?? '').toLowerCase()
  if (state.includes('disconnect')) return 'error'
  if (!lastPreviewUpdateMs) return state.includes('connect') ? 'lagging' : 'stale'

  const ageMs = Math.max(nowMs - lastPreviewUpdateMs, 0)
  const cadenceMs = timeframeToMs(timeframe) ?? 60_000
  if (ageMs <= cadenceMs) return 'live'
  if (ageMs <= cadenceMs * 2) return 'lagging'
  return 'stale'
}

export function freshnessLabel(freshness: Freshness): string {
  switch (freshness) {
    case 'ok':
      return 'healthy'
    case 'warn':
      return 'lagging'
    case 'stale':
      return 'stale'
    case 'error':
      return 'error'
  }
}

export function previewHealthLabel(health: PreviewHealth): string {
  switch (health) {
    case 'off':
      return 'off'
    case 'live':
      return 'live'
    case 'lagging':
      return 'lagging'
    case 'stale':
      return 'stale'
    case 'error':
      return 'error'
  }
}
