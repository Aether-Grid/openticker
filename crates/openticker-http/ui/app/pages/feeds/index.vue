<script setup lang="ts">
import type { StreamStatus } from '~/types/api'
import type { Freshness } from '~/utils/format'
import { fmtNumber, fmtRelative, freshnessLabel, streamFreshness } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const streams = shallowRef<StreamStatus[]>([])
const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)
const search = ref('')
const freshnessFilter = ref<'all' | Freshness>('all')

const load = async () => {
  pending.value = true
  try {
    streams.value = await api<StreamStatus[]>('/v1/data/streams')
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})

const freshnessItems = [
  { label: 'All', value: 'all' },
  { label: 'Healthy', value: 'ok' },
  { label: 'Lagging', value: 'warn' },
  { label: 'Stale', value: 'stale' },
  { label: 'Error', value: 'error' }
]

function streamState(s: StreamStatus): Freshness {
  return streamFreshness(s.staleness_ms, s.last_error)
}

const filtered = computed(() => {
  const term = search.value.trim().toLowerCase()
  return streams.value.filter((s) => {
    const state = streamState(s)
    if (term) {
      const hay = [s.key.symbol, s.key.account_id, s.key.timeframe].join(' ').toLowerCase()
      if (!hay.includes(term)) return false
    }
    if (freshnessFilter.value !== 'all' && state !== freshnessFilter.value) return false
    return true
  })
})

const kpis = computed(() => {
  const total = streams.value.length
  let healthy = 0
  let warn = 0
  let errors = 0
  streams.value.forEach((s) => {
    switch (streamState(s)) {
      case 'ok':
        healthy++
        break
      case 'warn':
        warn++
        break
      case 'stale':
        warn++
        break
      case 'error':
        errors++
        break
    }
  })
  return { total, healthy, warn, errors }
})

function sparkline(s: StreamStatus): number[] {
  if (Array.isArray(s.sparkline)) return s.sparkline.filter((v) => typeof v === 'number') as number[]
  return []
}

function freshnessTone(state: Freshness): 'green' | 'yellow' | 'red' | 'neutral' {
  switch (state) {
    case 'ok':
      return 'green'
    case 'warn':
      return 'yellow'
    case 'stale':
      return 'yellow'
    case 'error':
      return 'red'
  }
}

function feedHref(s: StreamStatus): string {
  return `/feeds/${encodeURIComponent(s.key.account_id)}/${encodeURIComponent(s.key.symbol)}/${encodeURIComponent(s.key.timeframe)}`
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Feeds</span>
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
      <div class="px-8 py-10 max-w-[1400px] mx-auto w-full space-y-10">
        <PageHeader
          eyebrow="Market data"
          title="The pulse of your strategies."
          description="Every subscribed stream, with freshness and a quick price shape."
        />

        <section class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <MetricCard
            label="Streams"
            :value="kpis.total"
          />
          <MetricCard
            label="Healthy"
            :value="kpis.healthy"
            accent="green"
          />
          <MetricCard
            label="Lagging"
            :value="kpis.warn"
            :accent="kpis.warn ? 'yellow' : 'neutral'"
          />
          <MetricCard
            label="Errors"
            :value="kpis.errors"
            :accent="kpis.errors ? 'red' : 'neutral'"
          />
        </section>

        <section class="space-y-4">
          <div class="flex flex-wrap items-center gap-3">
            <UInput
              v-model="search"
              icon="i-lucide-search"
              placeholder="Search symbol, account..."
              size="sm"
              class="flex-1 min-w-[240px]"
            />
            <USelect
              v-model="freshnessFilter"
              :items="freshnessItems"
              size="sm"
              class="w-48"
            />
            <div class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data ml-auto">
              {{ filtered.length }} / {{ streams.length }}
            </div>
          </div>

          <EmptyState
            v-if="!filtered.length"
            icon="i-lucide-radio"
            title="No streams yet"
            description="Configure a data connector to start buffering bars."
          />

          <div
            v-else
            class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3"
          >
            <NuxtLink
              v-for="(s, idx) in filtered"
              :key="`${s.key.account_id}-${s.key.symbol}-${s.key.timeframe}-${idx}`"
              :to="feedHref(s)"
              class="surface-card p-5 hover:bg-[color:var(--color-accent-softer)] hover:border-[color:var(--color-accent)]/20 transition-all flex flex-col gap-3"
            >
              <div class="flex items-start justify-between gap-3">
                <div>
                  <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">
                    {{ s.key.account_id }}
                  </div>
                  <div class="font-editorial text-[24px] leading-tight">
                    {{ s.key.symbol }}
                  </div>
                  <div class="text-[11px] text-ink-soft font-data mt-0.5">
                    {{ s.key.timeframe }}
                  </div>
                </div>
                <StatusPill
                  :label="freshnessLabel(streamState(s))"
                  :tone="freshnessTone(streamState(s))"
                />
              </div>

              <Sparkline
                :values="sparkline(s)"
                :width="260"
                :height="44"
                area
              />

              <div class="flex items-end justify-between">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Close</div>
                  <div class="font-data tabular-nums text-[18px]">
                    {{ fmtNumber(s.latest_bar?.close ?? 0, 4) }}
                  </div>
                </div>
                <div class="text-right">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Bar</div>
                  <div class="text-[12px] font-data">
                    {{ fmtRelative(s.latest_bar?.timestamp) }}
                  </div>
                </div>
              </div>

              <div class="flex items-center justify-between text-[11px] text-ink-soft font-data pt-2 hairline-t">
                <span>Bots: {{ s.attached_instances?.length ?? 0 }}</span>
                <span>Errors: {{ s.error_count ?? 0 }}</span>
              </div>
            </NuxtLink>
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
