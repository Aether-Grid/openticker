<script setup lang="ts">
import type {
  FillRecord,
  IntentRecord,
  OrderRecord,
  PositionRecord,
  ReconciliationRecord,
  RiskDecisionRecord,
  RuntimeEvent,
  SignalRecord
} from '~/types/api'
import { extractItems, fmtNumber, fmtPnL, fmtRelativeMs, pnlColor, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

type TabKey = 'events' | 'signals' | 'intents' | 'risk' | 'orders' | 'fills' | 'positions' | 'reconciliations'

type StoreValue =
  | RuntimeEvent[]
  | SignalRecord[]
  | IntentRecord[]
  | RiskDecisionRecord[]
  | OrderRecord[]
  | FillRecord[]
  | PositionRecord[]
  | ReconciliationRecord[]

const route = useRoute()
const router = useRouter()
const { api } = useApi()

const ALLOWED_TABS: TabKey[] = [
  'events',
  'signals',
  'intents',
  'risk',
  'orders',
  'fills',
  'positions',
  'reconciliations'
]

function resolveInitialTab(): TabKey {
  const q = route.query.tab
  const value = typeof q === 'string' ? q : Array.isArray(q) ? q[0] : null
  return ALLOWED_TABS.includes(value as TabKey) ? (value as TabKey) : 'signals'
}

const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)
const limit = ref<'60' | '120' | '250' | '500' | '1000'>('250')
const search = ref('')
const botFilter = ref('all')
const symbolFilter = ref('all')
const tab = ref<TabKey>(resolveInitialTab())

watch(tab, (value) => {
  void router.replace({ query: { ...route.query, tab: value } })
})

const data = reactive<Record<TabKey, StoreValue>>({
  events: [] as RuntimeEvent[],
  signals: [] as SignalRecord[],
  intents: [] as IntentRecord[],
  risk: [] as RiskDecisionRecord[],
  orders: [] as OrderRecord[],
  fills: [] as FillRecord[],
  positions: [] as PositionRecord[],
  reconciliations: [] as ReconciliationRecord[]
})

const load = async () => {
  pending.value = true
  try {
    const l = limit.value
    const [events, signals, intents, risk, orders, fills, positions, recon] = await Promise.all([
      api<RuntimeEvent[]>(`/v1/events?limit=${l}`).catch(() => [] as RuntimeEvent[]),
      api<SignalRecord[]>(`/v1/signals?limit=${l}`).catch(() => [] as SignalRecord[]),
      api<IntentRecord[]>(`/v1/intents?limit=${l}`).catch(() => [] as IntentRecord[]),
      api<{ items: RiskDecisionRecord[] } | RiskDecisionRecord[]>(`/v1/risk-decisions?limit=${l}`).catch(
        () => [] as RiskDecisionRecord[]
      ),
      api<OrderRecord[]>(`/v1/orders?limit=${l}`).catch(() => [] as OrderRecord[]),
      api<FillRecord[]>(`/v1/fills?limit=${l}`).catch(() => [] as FillRecord[]),
      api<PositionRecord[]>(`/v1/positions?limit=${l}`).catch(() => [] as PositionRecord[]),
      api<ReconciliationRecord[]>(`/v1/reconciliations?limit=${l}`).catch(() => [] as ReconciliationRecord[])
    ])
    data.events = Array.isArray(events) ? events : []
    data.signals = Array.isArray(signals) ? signals : []
    data.intents = Array.isArray(intents) ? intents : []
    data.risk = extractItems<RiskDecisionRecord>(risk)
    data.orders = Array.isArray(orders) ? orders : []
    data.fills = Array.isArray(fills) ? fills : []
    data.positions = Array.isArray(positions) ? positions : []
    data.reconciliations = Array.isArray(recon) ? recon : []
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})
watch(limit, () => {
  void load()
})

const current = computed(() => data[tab.value] as Array<Record<string, unknown>>)

const tabs = computed(() => [
  { label: 'Signals', value: 'signals' as TabKey, count: data.signals.length, icon: 'i-lucide-sparkles' },
  { label: 'Intents', value: 'intents' as TabKey, count: data.intents.length, icon: 'i-lucide-compass' },
  { label: 'Risk', value: 'risk' as TabKey, count: data.risk.length, icon: 'i-lucide-shield-alert' },
  { label: 'Orders', value: 'orders' as TabKey, count: data.orders.length, icon: 'i-lucide-send' },
  { label: 'Fills', value: 'fills' as TabKey, count: data.fills.length, icon: 'i-lucide-receipt' },
  { label: 'Positions', value: 'positions' as TabKey, count: data.positions.length, icon: 'i-lucide-layers' },
  {
    label: 'Reconciliations',
    value: 'reconciliations' as TabKey,
    count: data.reconciliations.length,
    icon: 'i-lucide-scale'
  },
  { label: 'Events', value: 'events' as TabKey, count: data.events.length, icon: 'i-lucide-radio-tower' }
])

function recordBotId(r: Record<string, unknown>): string | undefined {
  if (typeof r.bot_id === 'string') return r.bot_id
  if (typeof r.entity_id === 'string' && r.scope === 'bot') return r.entity_id
  return undefined
}

const bots = computed(() => {
  const set = new Set<string>()
  current.value.forEach((r) => {
    const id = recordBotId(r)
    if (id) set.add(id)
  })
  return Array.from(set).sort()
})

const symbols = computed(() => {
  const set = new Set<string>()
  current.value.forEach((r) => {
    if (typeof r.symbol === 'string') set.add(r.symbol)
  })
  return Array.from(set).sort()
})

const filtered = computed(() => {
  const term = search.value.trim().toLowerCase()
  return current.value
    .filter((r) => {
      const botId = recordBotId(r)
      if (botFilter.value !== 'all' && botId !== botFilter.value) return false
      if (symbolFilter.value !== 'all' && r.symbol !== symbolFilter.value) return false
      if (term) {
        const hay = JSON.stringify(r).toLowerCase()
        if (!hay.includes(term)) return false
      }
      return true
    })
    .sort((a, b) => ((b.created_at_ms as number) ?? 0) - ((a.created_at_ms as number) ?? 0))
})

function labelize(value: unknown): string {
  if (value === null || value === undefined) return '—'
  return String(value).replace(/_/g, ' ')
}

function signalTone(signal?: string | null): 'green' | 'red' | 'violet' | 'neutral' {
  const s = (signal ?? '').toLowerCase()
  if (s.startsWith('buy')) return 'green'
  if (s.startsWith('sell')) return 'red'
  if (!s || s === 'none' || s === 'no_op') return 'neutral'
  return 'violet'
}

function intentTone(intent?: string | null): 'green' | 'red' | 'blue' | 'neutral' {
  const s = (intent ?? '').toLowerCase()
  if (s === 'open_long' || s === 'add_long') return 'green'
  if (s === 'reduce_long' || s === 'close_long') return 'red'
  if (!s || s === 'no_op') return 'neutral'
  return 'blue'
}

function orderTone(status?: string | null): 'green' | 'yellow' | 'blue' | 'red' | 'neutral' {
  const s = (status ?? '').toLowerCase()
  if (s.includes('fill')) return 'green'
  if (s.includes('partial')) return 'yellow'
  if (s.includes('reject') || s.includes('cancel') || s.includes('expired')) return 'red'
  if (s.includes('open') || s.includes('working') || s.includes('pending') || s.includes('new')) return 'blue'
  return 'neutral'
}

function eventTone(scope?: string | null): 'accent' | 'blue' | 'violet' | 'neutral' {
  const s = (scope ?? '').toLowerCase()
  if (s === 'bot') return 'accent'
  if (s === 'service') return 'violet'
  if (s === 'connector') return 'blue'
  return 'neutral'
}

type Accent = 'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'orange' | 'teal' | 'accent' | 'neutral'
interface KpiEntry {
  label: string
  value: number | string
  accent?: Accent
}

function tabKpis(tabKey: TabKey): KpiEntry[] {
  const list = data[tabKey]
  switch (tabKey) {
    case 'signals': {
      const buys = (list as SignalRecord[]).filter((r) => r.signal.startsWith('buy')).length
      const sells = (list as SignalRecord[]).filter((r) => r.signal.startsWith('sell')).length
      return [
        { label: 'Total', value: list.length },
        { label: 'Buy', value: buys, accent: 'green' },
        { label: 'Sell', value: sells, accent: 'red' }
      ]
    }
    case 'intents': {
      const opens = (list as IntentRecord[]).filter((r) => r.intent === 'open_long' || r.intent === 'add_long').length
      const closes = (list as IntentRecord[]).filter(
        (r) => r.intent === 'close_long' || r.intent === 'reduce_long'
      ).length
      return [
        { label: 'Total', value: list.length },
        { label: 'Open / add', value: opens, accent: 'green' },
        { label: 'Reduce / close', value: closes, accent: 'red' }
      ]
    }
    case 'risk': {
      const allowed = (list as RiskDecisionRecord[]).filter((r) => r.decision === 'allowed').length
      const rejected = list.length - allowed
      return [
        { label: 'Total', value: list.length },
        { label: 'Allowed', value: allowed, accent: 'green' },
        { label: 'Rejected', value: rejected, accent: rejected ? 'red' : 'neutral' }
      ]
    }
    case 'orders': {
      const filled = (list as OrderRecord[]).filter((r) => r.status.toLowerCase().includes('fill')).length
      const working = list.length - filled
      return [
        { label: 'Total', value: list.length },
        { label: 'Filled', value: filled, accent: 'green' },
        { label: 'Working', value: working, accent: working ? 'blue' : 'neutral' }
      ]
    }
    case 'fills': {
      const totalQty = (list as FillRecord[]).reduce((acc, f) => acc + (f.quantity ?? 0), 0)
      const notional = (list as FillRecord[]).reduce((acc, f) => acc + (f.price ?? 0) * (f.quantity ?? 0), 0)
      return [
        { label: 'Total', value: list.length },
        { label: 'Total qty', value: fmtNumber(totalQty, 4) },
        { label: 'Notional', value: fmtNumber(notional, 2) }
      ]
    }
    case 'positions': {
      const open = (list as PositionRecord[]).filter((r) => r.has_position).length
      const realized = (list as PositionRecord[]).reduce((acc, r) => acc + (r.realized_pnl_usd ?? 0), 0)
      return [
        { label: 'Total', value: list.length },
        { label: 'Open states', value: open, accent: 'green' },
        { label: 'Realized PnL', value: fmtPnL(realized), accent: realized >= 0 ? 'green' : 'red' }
      ]
    }
    case 'reconciliations': {
      const safe = (list as ReconciliationRecord[]).filter((r) => r.safe_to_trade).length
      const blocked = list.length - safe
      return [
        { label: 'Total', value: list.length },
        { label: 'Safe', value: safe, accent: 'green' },
        { label: 'Blocked', value: blocked, accent: blocked ? 'red' : 'neutral' }
      ]
    }
    case 'events':
    default: {
      const scopes = new Set((list as RuntimeEvent[]).map((e) => e.scope))
      return [
        { label: 'Total', value: list.length },
        { label: 'Scopes', value: scopes.size, accent: 'accent' },
        { label: 'Kinds', value: new Set((list as RuntimeEvent[]).map((e) => e.kind)).size, accent: 'violet' }
      ]
    }
  }
}

const currentKpis = computed(() => tabKpis(tab.value))

function openDetail(id: number) {
  void router.push(`/activity/${tab.value}/${id}`)
}
</script>

<template>
  <UDashboardPanel>
    <template #header>
      <UDashboardNavbar :ui="{ root: 'border-b border-[color:var(--color-hairline)] px-8 h-14' }">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #title>
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Activity</span>
        </template>
        <template #right>
          <GlobalToolbar
            :last-updated="lastLoadedAt"
            :pending="pending"
            @refresh="load"
          />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1500px] mx-auto w-full space-y-8">
        <PageHeader
          eyebrow="Activity"
          title="Every signal, order, and reconciliation."
          description="A structured log of runtime events — filter, inspect, trace back to the cycle."
        />

        <section class="flex flex-wrap items-center gap-3">
          <UInput
            v-model="search"
            icon="i-lucide-search"
            placeholder="Search bot, symbol, reason, payload..."
            size="sm"
            class="flex-1 min-w-[280px]"
          />
          <USelect
            v-model="botFilter"
            :items="[{ label: 'All bots', value: 'all' }, ...bots.map((b) => ({ label: b, value: b }))]"
            size="sm"
            class="w-44"
          />
          <USelect
            v-model="symbolFilter"
            :items="[{ label: 'All symbols', value: 'all' }, ...symbols.map((s) => ({ label: s, value: s }))]"
            size="sm"
            class="w-40"
          />
          <USelect
            v-model="limit"
            :items="[
              { label: '60 records', value: '60' },
              { label: '120 records', value: '120' },
              { label: '250 records', value: '250' },
              { label: '500 records', value: '500' },
              { label: '1000 records', value: '1000' }
            ]"
            size="sm"
            class="w-40"
          />
          <UButton
            size="xs"
            color="neutral"
            variant="ghost"
            icon="i-lucide-x"
            label="Clear filters"
            @click="
              () => {
                search = ''
                botFilter = 'all'
                symbolFilter = 'all'
              }
            "
          />
        </section>

        <section class="flex flex-wrap gap-1.5">
          <button
            v-for="t in tabs"
            :key="t.value"
            type="button"
            class="inline-flex items-center gap-2 px-3 py-1.5 rounded-md text-[12px] tracking-tight transition-colors border"
            :class="
              tab === t.value
                ? 'bg-[color:var(--color-accent-soft)] border-[color:var(--color-accent)]/30 text-[color:var(--color-accent-ink)]'
                : 'surface-card hover:bg-[color:var(--color-accent-softer)]'
            "
            @click="tab = t.value"
          >
            <UIcon
              :name="t.icon"
              class="size-3.5"
              :class="tab === t.value ? 'text-[color:var(--color-accent-strong)]' : 'text-ink-soft'"
            />
            {{ t.label }}
            <span
              class="text-[10.5px] font-data tabular-nums opacity-80 rounded-full px-1.5 py-0.5 min-w-[20px] text-center"
              :class="tab === t.value ? 'bg-[color:var(--color-accent-strong)]/10' : 'bg-[color:var(--color-canvas)]'"
              >{{ t.count }}</span
            >
          </button>
        </section>

        <section class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <MetricCard
            v-for="(m, idx) in currentKpis"
            :key="`${tab}-${m.label}-${idx}`"
            :label="m.label"
            :value="m.value"
            :accent="m.accent ?? 'neutral'"
          />
          <MetricCard
            label="Window"
            :value="`${filtered.length} / ${current.length}`"
            accent="accent"
            :hint="`${tab} stream`"
          />
        </section>

        <section class="space-y-3">
          <EmptyState
            v-if="!filtered.length"
            icon="i-lucide-activity"
            title="No records match"
            description="Adjust filters or wait for new records."
          />
          <div
            v-else
            class="surface-card overflow-hidden"
          >
            <div class="divide-y divide-[color:var(--color-hairline)]">
              <template v-if="tab === 'signals'">
                <button
                  v-for="r in filtered as unknown as SignalRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-3">
                    <StatusPill
                      :label="labelize(r.signal)"
                      :tone="signalTone(r.signal)"
                    />
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="labelize(r.phase)"
                      :tone="r.phase === 'confirmed' ? 'accent' : 'neutral'"
                    />
                  </div>
                  <div class="col-span-2 text-right font-data tabular-nums text-[12.5px]">
                    {{ fmtNumber(r.close, 4) }}
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'intents'">
                <button
                  v-for="r in filtered as unknown as IntentRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-3 flex items-center gap-1.5">
                    <StatusPill
                      :label="labelize(r.signal)"
                      :tone="signalTone(r.signal)"
                    />
                    <UIcon
                      name="i-lucide-arrow-right"
                      class="size-3 text-ink-soft"
                    />
                    <StatusPill
                      :label="labelize(r.intent)"
                      :tone="intentTone(r.intent)"
                    />
                  </div>
                  <div class="col-span-4 text-[12px] text-ink-soft truncate">
                    {{ r.strategy_rationale ?? (r.has_position_before ? 'had position' : 'flat') }}
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'risk'">
                <button
                  v-for="r in filtered as unknown as RiskDecisionRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  :class="r.decision === 'rejected' ? 'border-l-2 border-l-[color:var(--color-pastel-red-ink)]' : ''"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="labelize(r.intent)"
                      :tone="intentTone(r.intent)"
                    />
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="r.decision"
                      :tone="r.decision === 'allowed' ? 'green' : 'red'"
                    />
                  </div>
                  <div class="col-span-3 text-[12px] text-ink-soft truncate">
                    {{ r.reason ?? '—' }}
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'orders'">
                <button
                  v-for="r in filtered as unknown as OrderRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="labelize(r.intent)"
                      :tone="intentTone(r.intent)"
                    />
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="r.status"
                      :tone="orderTone(r.status)"
                    />
                  </div>
                  <div class="col-span-3 text-right">
                    <div class="font-data tabular-nums text-[13px]">
                      {{ fmtNumber(r.quantity, 4) }} × {{ fmtNumber(r.price, 4) }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ short(r.client_order_id, 20) }}
                    </div>
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'fills'">
                <button
                  v-for="r in filtered as unknown as FillRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-3 text-right">
                    <div class="font-data tabular-nums text-[13px]">
                      {{ fmtNumber(r.quantity, 4) }} × {{ fmtNumber(r.price, 4) }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data">
                      notional {{ fmtNumber(r.price * r.quantity, 2) }}
                    </div>
                  </div>
                  <div class="col-span-2">
                    <div
                      v-if="r.fee_amount !== undefined && r.fee_amount !== null"
                      class="text-[12px] font-data"
                    >
                      {{ fmtNumber(r.fee_amount, 4) }} <span class="text-ink-soft">{{ r.fee_asset }}</span>
                    </div>
                    <div
                      v-else
                      class="text-[11px] text-ink-soft font-data"
                    >
                      no fee
                    </div>
                  </div>
                  <div class="col-span-2 text-[11px] text-ink-soft font-data truncate">
                    {{ short(r.client_order_id, 18) }}
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'positions'">
                <button
                  v-for="r in filtered as unknown as PositionRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol ?? '—' }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.bot_id }}
                    </div>
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="r.has_position ? 'open' : 'flat'"
                      :tone="r.has_position ? 'teal' : 'neutral'"
                    />
                  </div>
                  <div class="col-span-2 text-right font-data tabular-nums text-[12.5px]">
                    {{ fmtNumber(r.quantity, 4) }}
                  </div>
                  <div class="col-span-2 text-right font-data tabular-nums text-[12.5px]">
                    {{ r.entry_price !== undefined && r.entry_price !== null ? fmtNumber(r.entry_price, 4) : '—' }}
                  </div>
                  <div
                    class="col-span-2 text-right font-data tabular-nums text-[13px]"
                    :class="pnlColor(r.realized_pnl_usd)"
                  >
                    {{ fmtPnL(r.realized_pnl_usd) }}
                  </div>
                  <div
                    class="col-span-1 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else-if="tab === 'reconciliations'">
                <button
                  v-for="r in filtered as unknown as ReconciliationRecord[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  :class="r.safe_to_trade ? '' : 'border-l-2 border-l-[color:var(--color-pastel-red-ink)]'"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-3">
                    <div class="font-editorial text-[15px] leading-tight">
                      {{ r.symbol }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">{{ r.bot_id }} · {{ r.source }}</div>
                  </div>
                  <div class="col-span-2">
                    <StatusPill
                      :label="r.safe_to_trade ? 'safe' : 'blocked'"
                      :tone="r.safe_to_trade ? 'green' : 'red'"
                    />
                  </div>
                  <div class="col-span-3 text-[12px] text-ink-soft font-data">
                    local <span class="text-ink">{{ r.local_open_orders }}</span> / conn
                    <span class="text-ink">{{ r.connector_open_orders }}</span>
                  </div>
                  <div class="col-span-2 text-[12px] text-ink-soft font-data">
                    pos
                    <span class="text-ink"
                      >{{ r.local_has_position ? 'Y' : 'N' }}/{{ r.connector_has_position ? 'Y' : 'N' }}</span
                    >
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>

              <template v-else>
                <button
                  v-for="r in filtered as unknown as RuntimeEvent[]"
                  :key="r.id"
                  type="button"
                  class="w-full text-left px-5 py-3 grid grid-cols-12 gap-3 items-center transition-colors hover:bg-[color:var(--color-accent-softer)]"
                  @click="openDetail(r.id)"
                >
                  <div class="col-span-2">
                    <StatusPill
                      :label="r.scope"
                      :tone="eventTone(r.scope)"
                    />
                  </div>
                  <div class="col-span-3">
                    <div class="font-data text-[12.5px] font-medium truncate">
                      {{ r.kind }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data truncate">
                      {{ r.entity_id ?? '—' }}
                    </div>
                  </div>
                  <div class="col-span-5 text-[12px] text-ink-soft font-data truncate">
                    {{ short(r.payload, 100) }}
                  </div>
                  <div
                    class="col-span-2 text-right text-[11px] text-ink-soft font-data flex items-center justify-end gap-1.5"
                  >
                    {{ fmtRelativeMs(r.created_at_ms) }}
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="size-3 text-ink-soft/60"
                    />
                  </div>
                </button>
              </template>
            </div>
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
