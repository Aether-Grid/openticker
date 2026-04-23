<script setup lang="ts">
import type { RuntimeEvent } from '~/types/api'
import { extractItems, fmtDateTimeMs, fmtRelativeMs, short, tryParseJson } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { snapshot, pending, lastLoadedAt, refresh } = useDashboardSnapshot(200)

type Stage = 'requested' | 'received' | 'normalized' | 'succeeded' | 'failed' | 'ignored' | 'unknown'

interface ProviderPayload {
  connector_kind?: string
  operation?: string
  stage?: Stage | string
  summary?: string
  account_id?: string
  [key: string]: unknown
}

interface ProviderEvent extends RuntimeEvent {
  payloadJson: ProviderPayload | null
  connector: string
  operation: string
  stage: Stage
  summary: string
  account: string
}

function deriveStage(event: RuntimeEvent, payload: ProviderPayload | null): Stage {
  if (payload && typeof payload.stage === 'string') return payload.stage as Stage
  const kind = event.kind ?? ''
  const tail = kind.includes('.') ? (kind.split('.').pop() ?? '') : ''
  if (['requested', 'received', 'normalized', 'succeeded', 'failed', 'ignored'].includes(tail)) {
    return tail as Stage
  }
  return 'unknown'
}

function deriveOperation(event: RuntimeEvent, payload: ProviderPayload | null): string {
  if (payload && typeof payload.operation === 'string') return payload.operation
  const kind = event.kind ?? ''
  if (kind.includes('market_stream')) return 'market_stream'
  if (kind.includes('account_snapshot')) return 'fetch_account_snapshot'
  if (kind.includes('symbol_constraints')) return 'fetch_symbol_constraints'
  if (kind.includes('order_submission')) return 'submit_order'
  if (kind.includes('market_data.latest')) return 'fetch_latest_bar'
  if (kind.includes('market_data.history')) return 'fetch_recent_bars'
  return kind.split('.')[0] || 'provider_event'
}

const rawEvents = computed<RuntimeEvent[]>(
  () => extractItems(snapshot.value?.providerEvents) as unknown as RuntimeEvent[]
)

const events = computed<ProviderEvent[]>(() =>
  rawEvents.value.map((event) => {
    const parsed = tryParseJson(event.payload)
    const payloadJson: ProviderPayload | null =
      parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? (parsed as ProviderPayload) : null
    const stage = deriveStage(event, payloadJson)
    const operation = deriveOperation(event, payloadJson)
    const connector = payloadJson?.connector_kind ?? 'connector'
    const summary =
      payloadJson?.summary ?? (typeof event.payload === 'string' ? event.payload : JSON.stringify(event.payload))
    const account = payloadJson?.account_id ?? event.entity_id ?? 'service'
    return {
      ...event,
      payloadJson,
      connector,
      operation,
      stage,
      summary,
      account
    }
  })
)

const search = ref('')
const connectorFilter = ref('all')
const operationFilter = ref('all')
const stageFilter = ref<'all' | Stage>('all')
const selectedId = ref<number | null>(null)

const connectors = computed(() => Array.from(new Set(events.value.map((e) => e.connector))).sort())
const operations = computed(() => Array.from(new Set(events.value.map((e) => e.operation))).sort())

const filtered = computed(() => {
  const term = search.value.trim().toLowerCase()
  return events.value.filter((e) => {
    if (connectorFilter.value !== 'all' && e.connector !== connectorFilter.value) return false
    if (operationFilter.value !== 'all' && e.operation !== operationFilter.value) return false
    if (stageFilter.value !== 'all' && e.stage !== stageFilter.value) return false
    if (term) {
      const hay =
        `${e.connector} ${e.operation} ${e.stage} ${e.summary} ${e.account} ${e.kind} ${e.entity_id ?? ''}`.toLowerCase()
      if (!hay.includes(term)) return false
    }
    return true
  })
})

const selected = computed<ProviderEvent | null>(() => {
  if (!filtered.value.length) return null
  if (selectedId.value !== null) {
    const found = filtered.value.find((e) => e.id === selectedId.value)
    if (found) return found
  }
  return filtered.value[0] ?? null
})

watch(filtered, (list) => {
  if (!list.length) {
    selectedId.value = null
    return
  }
  if (selectedId.value === null || !list.some((e) => e.id === selectedId.value)) {
    const first = list[0]
    selectedId.value = first ? first.id : null
  }
})

function stageTone(stage: Stage): 'green' | 'red' | 'yellow' | 'blue' | 'violet' | 'neutral' {
  switch (stage) {
    case 'succeeded':
    case 'normalized':
      return 'green'
    case 'failed':
      return 'red'
    case 'ignored':
      return 'yellow'
    case 'requested':
    case 'received':
      return 'blue'
    default:
      return 'neutral'
  }
}

function stageBg(stage: Stage): string {
  switch (stage) {
    case 'succeeded':
    case 'normalized':
      return 'var(--color-pastel-green-ink)'
    case 'failed':
      return 'var(--color-pastel-red-ink)'
    case 'ignored':
      return 'var(--color-pastel-yellow-ink)'
    case 'requested':
    case 'received':
      return 'var(--color-pastel-blue-ink)'
    default:
      return 'var(--color-ink-soft)'
  }
}

function operationIcon(operation: string): string {
  if (operation.includes('market_stream')) return 'i-lucide-radio'
  if (operation.includes('submit_order')) return 'i-lucide-send'
  if (operation.includes('account_snapshot')) return 'i-lucide-wallet'
  if (operation.includes('symbol_constraints')) return 'i-lucide-ruler'
  if (operation.includes('latest_bar')) return 'i-lucide-activity'
  if (operation.includes('recent_bars')) return 'i-lucide-history'
  return 'i-lucide-plug'
}

interface OperationMix {
  operation: string
  total: number
  succeeded: number
  failed: number
  inflight: number
  ignored: number
}

const operationMix = computed<OperationMix[]>(() => {
  const map = new Map<string, OperationMix>()
  for (const event of events.value) {
    const row = map.get(event.operation) ?? {
      operation: event.operation,
      total: 0,
      succeeded: 0,
      failed: 0,
      inflight: 0,
      ignored: 0
    }
    row.total++
    switch (event.stage) {
      case 'succeeded':
      case 'normalized':
        row.succeeded++
        break
      case 'failed':
        row.failed++
        break
      case 'ignored':
        row.ignored++
        break
      case 'requested':
      case 'received':
        row.inflight++
        break
    }
    map.set(event.operation, row)
  }
  return Array.from(map.values()).sort((a, b) => b.total - a.total)
})

const maxOperationTotal = computed(() => operationMix.value.reduce((acc, row) => Math.max(acc, row.total), 1))

const kpis = computed(() => {
  const total = events.value.length
  let requested = 0
  let succeeded = 0
  let failed = 0
  let ignored = 0
  for (const event of events.value) {
    switch (event.stage) {
      case 'requested':
      case 'received':
        requested++
        break
      case 'succeeded':
      case 'normalized':
        succeeded++
        break
      case 'failed':
        failed++
        break
      case 'ignored':
        ignored++
        break
    }
  }
  return {
    total,
    requested,
    succeeded,
    failed,
    ignored,
    connectors: connectors.value.length
  }
})

const stageItems: Array<{ label: string; value: 'all' | Stage }> = [
  { label: 'All stages', value: 'all' },
  { label: 'Requested', value: 'requested' },
  { label: 'Received', value: 'received' },
  { label: 'Normalized', value: 'normalized' },
  { label: 'Succeeded', value: 'succeeded' },
  { label: 'Failed', value: 'failed' },
  { label: 'Ignored', value: 'ignored' }
]
</script>

<template>
  <UDashboardPanel>
    <template #header>
      <UDashboardNavbar :ui="{ root: 'border-b border-[color:var(--color-hairline)] px-8 h-14' }">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #title>
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Providers</span>
        </template>
        <template #right>
          <GlobalToolbar
            :last-updated="lastLoadedAt"
            :pending="pending"
            @refresh="refresh"
          />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1500px] mx-auto w-full space-y-8">
        <PageHeader
          eyebrow="Providers"
          title="Every request the runtime made on your behalf."
          description="Normalized provider events — requests, responses, failures — across data and execution connectors."
        />

        <section class="grid grid-cols-2 md:grid-cols-6 gap-3">
          <MetricCard
            label="Total"
            :value="kpis.total"
            hint="In buffer"
          />
          <MetricCard
            label="Requested"
            :value="kpis.requested"
            accent="blue"
            hint="Outbound or inbound"
          />
          <MetricCard
            label="Succeeded"
            :value="kpis.succeeded"
            accent="green"
            hint="Success or normalized"
          />
          <MetricCard
            label="Failed"
            :value="kpis.failed"
            :accent="kpis.failed ? 'red' : 'neutral'"
          />
          <MetricCard
            label="Ignored"
            :value="kpis.ignored"
            :accent="kpis.ignored ? 'yellow' : 'neutral'"
          />
          <MetricCard
            label="Connectors"
            :value="kpis.connectors"
            accent="accent"
          />
        </section>

        <section class="flex flex-wrap items-center gap-3">
          <UInput
            v-model="search"
            icon="i-lucide-search"
            placeholder="Search connector, operation, summary..."
            size="sm"
            class="flex-1 min-w-[280px]"
          />
          <USelect
            v-model="connectorFilter"
            :items="[{ label: 'All connectors', value: 'all' }, ...connectors.map((c) => ({ label: c, value: c }))]"
            size="sm"
            class="w-44"
          />
          <USelect
            v-model="operationFilter"
            :items="[
              { label: 'All operations', value: 'all' },
              ...operations.map((o) => ({ label: o.replace(/_/g, ' '), value: o }))
            ]"
            size="sm"
            class="w-56"
          />
          <USelect
            v-model="stageFilter"
            :items="stageItems"
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
                connectorFilter = 'all'
                operationFilter = 'all'
                stageFilter = 'all'
              }
            "
          />
          <div class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data ml-auto">
            {{ filtered.length }} / {{ events.length }}
          </div>
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Traffic mix"
            hint="Calls per operation, split by stage."
          />
          <div
            v-if="!operationMix.length"
            class="surface-card p-10 text-center text-ink-soft text-[13px]"
          >
            Nothing recorded yet.
          </div>
          <div
            v-else
            class="surface-card p-5 space-y-3"
          >
            <div
              v-for="row in operationMix"
              :key="row.operation"
              class="space-y-1.5"
            >
              <div class="flex items-center justify-between gap-3 text-[12px]">
                <div class="flex items-center gap-2 min-w-0">
                  <UIcon
                    :name="operationIcon(row.operation)"
                    class="size-3.5 text-ink-soft"
                  />
                  <span class="font-data truncate">{{ row.operation.replace(/_/g, ' ') }}</span>
                </div>
                <div class="flex items-center gap-3 text-[11px] font-data tabular-nums">
                  <span
                    v-if="row.succeeded"
                    class="text-[color:var(--color-pastel-green-ink)]"
                    >✓ {{ row.succeeded }}</span
                  >
                  <span
                    v-if="row.inflight"
                    class="text-[color:var(--color-pastel-blue-ink)]"
                    >↻ {{ row.inflight }}</span
                  >
                  <span
                    v-if="row.failed"
                    class="text-[color:var(--color-pastel-red-ink)]"
                    >✕ {{ row.failed }}</span
                  >
                  <span
                    v-if="row.ignored"
                    class="text-[color:var(--color-pastel-yellow-ink)]"
                    >↷ {{ row.ignored }}</span
                  >
                  <span class="text-ink font-medium">{{ row.total }}</span>
                </div>
              </div>
              <div class="h-1.5 rounded-full bg-[color:var(--color-canvas)] overflow-hidden flex">
                <div
                  v-if="row.succeeded"
                  :style="{
                    width: `${(row.succeeded / maxOperationTotal) * 100}%`,
                    backgroundColor: 'var(--color-pastel-green-ink)'
                  }"
                />
                <div
                  v-if="row.inflight"
                  :style="{
                    width: `${(row.inflight / maxOperationTotal) * 100}%`,
                    backgroundColor: 'var(--color-pastel-blue-ink)'
                  }"
                />
                <div
                  v-if="row.failed"
                  :style="{
                    width: `${(row.failed / maxOperationTotal) * 100}%`,
                    backgroundColor: 'var(--color-pastel-red-ink)'
                  }"
                />
                <div
                  v-if="row.ignored"
                  :style="{
                    width: `${(row.ignored / maxOperationTotal) * 100}%`,
                    backgroundColor: 'var(--color-pastel-yellow-ink)'
                  }"
                />
              </div>
            </div>
          </div>
        </section>

        <section class="grid grid-cols-12 gap-6">
          <div class="col-span-12 xl:col-span-7 space-y-3">
            <SectionHeader
              label="Event log"
              hint="Click a row to inspect."
            />
            <EmptyState
              v-if="!filtered.length"
              icon="i-lucide-plug"
              title="Provider bus is quiet"
              description="No calls have flown through in the current window."
            />
            <div
              v-else
              class="surface-card overflow-hidden"
            >
              <div class="max-h-[640px] overflow-y-auto scroll-fade-mask divide-y divide-[color:var(--color-hairline)]">
                <button
                  v-for="event in filtered"
                  :key="event.id"
                  type="button"
                  class="w-full text-left px-5 py-3 transition-colors border-l-2"
                  :class="[
                    selectedId === event.id
                      ? 'bg-[color:var(--color-accent-soft)]'
                      : 'hover:bg-[color:var(--color-accent-softer)]'
                  ]"
                  :style="{ borderLeftColor: stageBg(event.stage) }"
                  @click="selectedId = event.id"
                >
                  <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0 flex-1 space-y-1">
                      <div class="flex items-center gap-2 text-[12.5px] font-data font-medium">
                        <span class="text-ink truncate">{{ event.connector }}</span>
                        <UIcon
                          name="i-lucide-chevron-right"
                          class="size-3 text-ink-soft shrink-0"
                        />
                        <span class="text-ink-soft truncate">{{ event.operation.replace(/_/g, ' ') }}</span>
                        <StatusPill
                          :label="event.stage"
                          :tone="stageTone(event.stage)"
                        />
                      </div>
                      <div class="text-[12px] text-ink-soft leading-snug line-clamp-2">
                        {{ event.summary || 'no summary' }}
                      </div>
                      <div class="flex items-center gap-3 text-[11px] text-ink-soft font-data">
                        <span>{{ event.account }}</span>
                        <span>·</span>
                        <span>{{ event.kind }}</span>
                      </div>
                    </div>
                    <div class="text-right shrink-0 text-[11px] text-ink-soft font-data">
                      {{ fmtRelativeMs(event.created_at_ms) }}
                    </div>
                  </div>
                </button>
              </div>
            </div>
          </div>

          <div class="col-span-12 xl:col-span-5 space-y-3">
            <SectionHeader
              label="Inspector"
              hint="Selected event detail."
            />
            <div
              v-if="selected"
              class="space-y-3"
            >
              <div class="surface-card p-5 space-y-4 relative overflow-hidden">
                <div
                  class="absolute top-0 left-0 bottom-0 w-[3px]"
                  :style="{ backgroundColor: stageBg(selected.stage) }"
                />
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div
                      class="flex items-center gap-2 text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data"
                    >
                      <UIcon
                        :name="operationIcon(selected.operation)"
                        class="size-3"
                      />
                      <span>{{ selected.operation.replace(/_/g, ' ') }}</span>
                    </div>
                    <div class="font-editorial text-[22px] leading-tight mt-1 truncate">
                      {{ selected.connector }}
                    </div>
                    <div class="text-[11px] text-ink-soft font-data mt-0.5 truncate">
                      {{ selected.kind }} · #{{ selected.id }}
                    </div>
                  </div>
                  <StatusPill
                    :label="selected.stage"
                    :tone="stageTone(selected.stage)"
                  />
                </div>

                <div
                  v-if="selected.summary"
                  class="text-[13px] leading-snug text-ink"
                >
                  {{ selected.summary }}
                </div>

                <dl class="grid grid-cols-2 gap-3 text-[12.5px] pt-3 hairline-t">
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Connector</dt>
                    <dd class="mt-1 font-data truncate">
                      {{ selected.connector }}
                    </dd>
                  </div>
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Account</dt>
                    <dd class="mt-1 font-data truncate">
                      {{ selected.account }}
                    </dd>
                  </div>
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Operation</dt>
                    <dd class="mt-1 font-data truncate">
                      {{ selected.operation.replace(/_/g, ' ') }}
                    </dd>
                  </div>
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Stage</dt>
                    <dd class="mt-1">
                      <StatusPill
                        :label="selected.stage"
                        :tone="stageTone(selected.stage)"
                      />
                    </dd>
                  </div>
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Entity</dt>
                    <dd class="mt-1 font-data truncate">
                      {{ selected.entity_id ?? 'service' }}
                    </dd>
                  </div>
                  <div>
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Recorded</dt>
                    <dd class="mt-1 font-data">
                      {{ fmtRelativeMs(selected.created_at_ms) }}
                    </dd>
                    <dd class="text-[10.5px] text-ink-soft font-data mt-0.5">
                      {{ fmtDateTimeMs(selected.created_at_ms) }}
                    </dd>
                  </div>
                  <div
                    v-if="selected.trace_id"
                    class="col-span-2"
                  >
                    <dt class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Trace</dt>
                    <dd class="mt-1 font-data text-[12px] truncate">
                      {{ short(selected.trace_id, 52) }}
                    </dd>
                  </div>
                </dl>
              </div>

              <div
                v-if="selected.payloadJson"
                class="space-y-2"
              >
                <SectionHeader
                  label="Payload"
                  hint="Normalized provider envelope."
                />
                <JsonBlock
                  :value="selected.payloadJson"
                  max-height="360px"
                />
              </div>

              <div class="space-y-2">
                <SectionHeader
                  label="Raw event"
                  hint="Full event as stored in the journal."
                />
                <JsonBlock
                  :value="selected"
                  max-height="320px"
                />
              </div>
            </div>
            <EmptyState
              v-else
              icon="i-lucide-mouse-pointer-2"
              title="Select an event"
              description="Rows on the left will populate this pane."
            />
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
