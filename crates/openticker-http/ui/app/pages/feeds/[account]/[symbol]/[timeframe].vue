<script setup lang="ts">
import type { OhlcvBar, StreamStatus } from '~/types/api'
import type { PreviewHealth } from '~/utils/format'
import {
  closedBarFreshness,
  fmtDateTime,
  fmtNumber,
  fmtRelative,
  fmtRelativeMs,
  freshnessLabel,
  previewHealth,
  previewHealthLabel
} from '~/utils/format'

definePageMeta({ layout: 'default' })

const route = useRoute()
const account = computed(() => route.params.account as string)
const symbol = computed(() => route.params.symbol as string)
const timeframe = computed(() => route.params.timeframe as string)

const { api } = useApi()

const memoryBars = shallowRef<OhlcvBar[]>([])
const historyBars = shallowRef<OhlcvBar[]>([])
const streamStatus = shallowRef<StreamStatus | null>(null)
const pendingMem = ref(false)
const pendingHist = ref(false)
const lastLoadedAt = ref<Date | null>(null)

const streamPath = computed(
  () =>
    `/v1/data/streams/${encodeURIComponent(account.value)}/${encodeURIComponent(symbol.value)}/${encodeURIComponent(timeframe.value)}`
)

const loadMemory = async () => {
  pendingMem.value = true
  try {
    const data = await api<{ bars: OhlcvBar[] } | OhlcvBar[]>(`${streamPath.value}/bars?limit=200`)
    memoryBars.value = Array.isArray(data) ? data : (data.bars ?? [])
    const streams = await api<StreamStatus[]>('/v1/data/streams')
    streamStatus.value = streams.find((s) => streamMatchesRoute(s)) ?? null
    lastLoadedAt.value = new Date()
  } finally {
    pendingMem.value = false
  }
}
const loadHistory = async () => {
  pendingHist.value = true
  try {
    const data = await api<{ bars: OhlcvBar[] }>(`${streamPath.value}/history?limit=200`)
    historyBars.value = data.bars ?? []
  } finally {
    pendingHist.value = false
  }
}

useAutoRefresh(loadMemory)
watch(
  [account, symbol, timeframe],
  () => {
    void loadMemory()
  },
  { immediate: true }
)

const closes = computed(() => memoryBars.value.map((b) => b.close))
const latest = computed(() => memoryBars.value.at(-1) ?? null)
const high24 = computed(() => (memoryBars.value.length ? Math.max(...memoryBars.value.map((b) => b.high)) : null))
const low24 = computed(() => (memoryBars.value.length ? Math.min(...memoryBars.value.map((b) => b.low)) : null))
const vol = computed(() => memoryBars.value.reduce((acc, b) => acc + (b.volume ?? 0), 0))

const closedState = computed(() =>
  closedBarFreshness(
    streamStatus.value?.confirmed_bar_staleness_ms,
    streamStatus.value?.last_error,
    streamStatus.value?.polling_interval_ms,
    streamStatus.value?.close_poll_grace_ms,
    streamStatus.value?.transport_staleness_ms ?? streamStatus.value?.staleness_ms,
    timeframe.value
  )
)
const currentPreviewHealth = computed(() =>
  previewHealth(
    streamStatus.value?.preview_enabled,
    streamStatus.value?.preview_connection_state,
    streamStatus.value?.last_preview_update_ms,
    streamStatus.value?.last_preview_error,
    timeframe.value
  )
)

const barsTable = [
  { key: 'timestamp', label: 'Opened' },
  { key: 'open', label: 'Open', align: 'right' as const },
  { key: 'high', label: 'High', align: 'right' as const },
  { key: 'low', label: 'Low', align: 'right' as const },
  { key: 'close', label: 'Close', align: 'right' as const },
  { key: 'volume', label: 'Volume', align: 'right' as const }
]

function barField(row: OhlcvBar, key: string): number {
  const value = (row as unknown as Record<string, unknown>)[key]
  return typeof value === 'number' ? value : 0
}

function streamMatchesRoute(stream: StreamStatus): boolean {
  return (
    stream.key.account_id === account.value &&
    stream.key.symbol === symbol.value &&
    stream.key.timeframe === timeframe.value
  )
}

function freshnessTone(state: ReturnType<typeof closedBarFreshness>): 'green' | 'yellow' | 'red' | 'neutral' {
  if (state === 'ok') return 'green'
  if (state === 'error') return 'red'
  return 'yellow'
}

function previewTone(state: PreviewHealth): 'green' | 'yellow' | 'red' | 'neutral' {
  if (state === 'live') return 'green'
  if (state === 'error') return 'red'
  if (state === 'off') return 'neutral'
  return 'yellow'
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
          <div class="flex items-center gap-3">
            <NuxtLink
              to="/feeds"
              class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data hover:text-ink"
            >
              Feeds /
            </NuxtLink>
            <span class="font-data text-[13px]">{{ symbol }} · {{ timeframe }}</span>
          </div>
        </template>
        <template #right>
          <GlobalToolbar
            :last-updated="lastLoadedAt"
            :pending="pendingMem"
            @refresh="loadMemory"
          />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1400px] mx-auto w-full space-y-10">
        <PageHeader
          :eyebrow="account"
          :title="`${symbol} · ${timeframe}`"
          :description="latest ? `Latest bar opened ${fmtDateTime(latest.timestamp)}` : 'Awaiting first bar.'"
        >
          <template #actions>
            <StatusPill
              :label="`Closed ${freshnessLabel(closedState)}`"
              :tone="freshnessTone(closedState)"
            />
            <StatusPill
              v-if="streamStatus?.preview_enabled"
              :label="`Preview ${previewHealthLabel(currentPreviewHealth)}`"
              :tone="previewTone(currentPreviewHealth)"
            />
            <UButton
              color="neutral"
              variant="outline"
              size="sm"
              icon="i-lucide-cloud-download"
              label="Fetch history"
              :loading="pendingHist"
              @click="loadHistory"
            />
          </template>
        </PageHeader>

        <section class="grid grid-cols-2 md:grid-cols-4 xl:grid-cols-8 gap-3">
          <MetricCard
            label="Confirmed"
            :value="fmtNumber(latest?.close ?? 0, 4)"
            :hint="
              streamStatus?.confirmed_bar_close_ms
                ? `Closed ${fmtRelativeMs(streamStatus.confirmed_bar_close_ms)}`
                : 'Latest bar'
            "
          />
          <MetricCard
            label="Preview"
            :value="streamStatus?.preview_enabled ? fmtNumber(streamStatus?.latest_preview_bar?.close ?? 0, 4) : '—'"
            :hint="
              streamStatus?.preview_enabled
                ? `Updated ${fmtRelativeMs(streamStatus?.last_preview_update_ms)}`
                : 'Disabled'
            "
          />
          <MetricCard
            label="Open"
            :value="fmtNumber(latest?.open ?? 0, 4)"
          />
          <MetricCard
            label="High"
            :value="fmtNumber(high24 ?? 0, 4)"
            hint="Buffer high"
            accent="green"
          />
          <MetricCard
            label="Low"
            :value="fmtNumber(low24 ?? 0, 4)"
            hint="Buffer low"
            accent="red"
          />
          <MetricCard
            label="Volume"
            :value="fmtNumber(vol, 2)"
            hint="Buffer total"
          />
          <MetricCard
            label="Last poll"
            :value="fmtRelativeMs(streamStatus?.last_success_ms)"
            :hint="streamStatus?.last_error ?? 'Transport recency'"
            :accent="streamStatus?.last_error ? 'red' : 'neutral'"
          />
        </section>

        <section class="surface-card p-6">
          <SectionHeader
            label="Closes (buffered)"
            :hint="`${memoryBars.length} bars in memory`"
          />
          <Sparkline
            :values="closes"
            :width="1200"
            :height="160"
            area
          />
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-6 space-y-4">
            <SectionHeader
              label="In-memory bars"
              hint="Fed directly to bots from the ring buffer."
            />
            <DataTable
              :columns="barsTable"
              :rows="[...memoryBars].reverse()"
            >
              <template #cell="{ row, column }">
                <template v-if="column.key === 'timestamp'">
                  <span class="font-data text-[12px]">{{ fmtRelative(row.timestamp) }}</span>
                </template>
                <template v-else>
                  <span class="font-data tabular-nums text-[12.5px]">{{
                    fmtNumber(barField(row, column.key), 4)
                  }}</span>
                </template>
              </template>
            </DataTable>
          </div>

          <div class="col-span-12 xl:col-span-6 space-y-4">
            <SectionHeader
              label="Connector history"
              hint="On-demand fetch from the underlying connector."
            />
            <DataTable
              :columns="barsTable"
              :rows="[...historyBars].reverse()"
            >
              <template #cell="{ row, column }">
                <template v-if="column.key === 'timestamp'">
                  <span class="font-data text-[12px]">{{ fmtRelative(row.timestamp) }}</span>
                </template>
                <template v-else>
                  <span class="font-data tabular-nums text-[12.5px]">{{
                    fmtNumber(barField(row, column.key), 4)
                  }}</span>
                </template>
              </template>
              <template #empty> Click "Fetch history" to pull recent bars from the connector. </template>
            </DataTable>
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
