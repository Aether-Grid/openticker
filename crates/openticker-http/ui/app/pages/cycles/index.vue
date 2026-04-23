<script setup lang="ts">
import type { BotSummary, CycleSummary, CycleOutcome } from '~/types/api'
import { fmtDate, fmtRelative, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()

const bots = shallowRef<BotSummary[]>([])
const cycles = shallowRef<CycleSummary[]>([])
const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)

const search = ref('')
const botFilter = ref('all')
const symbolFilter = ref('all')
const phaseFilter = ref('all')
const outcomeFilter = ref<'all' | CycleOutcome>('all')

const load = async () => {
  pending.value = true
  try {
    bots.value = await api<BotSummary[]>('/v1/bots?limit=200')
    const results = await Promise.all(
      bots.value.slice(0, 30).map((bot) =>
        api<CycleSummary[]>(`/v1/bots/${encodeURIComponent(bot.id)}/cycles?limit=120`)
          .then((list) => (list || []).map((c) => ({ ...c, bot_id: c.bot_id ?? bot.id })))
          .catch(() => [] as CycleSummary[])
      )
    )
    cycles.value = results
      .flat()
      .filter((c) => !!c && !!c.trace_id)
      .sort((a, b) => b.created_at_ms - a.created_at_ms)
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})

const botItems = computed(() => [
  { label: 'All bots', value: 'all' },
  ...bots.value.map((b) => ({ label: b.display_name ?? b.id, value: b.id }))
])
const symbolItems = computed(() => {
  const set = new Set<string>()
  cycles.value.forEach((c) => c.symbol && set.add(c.symbol))
  return [
    { label: 'All symbols', value: 'all' },
    ...Array.from(set)
      .sort()
      .map((s) => ({ label: s, value: s }))
  ]
})
const phaseItems = [
  { label: 'All phases', value: 'all' },
  { label: 'Preview', value: 'preview' },
  { label: 'Confirmed', value: 'confirmed' }
]
const outcomeItems: Array<{ label: string; value: 'all' | CycleOutcome }> = [
  { label: 'All outcomes', value: 'all' },
  { label: 'Accepted · filled', value: 'accepted_filled' },
  { label: 'Accepted · partially filled', value: 'accepted_partially_filled' },
  { label: 'Accepted · no fill', value: 'accepted_no_fill' },
  { label: 'No-op', value: 'no_op' },
  { label: 'Risk rejected', value: 'risk_rejected' },
  { label: 'Failed', value: 'failed' }
]

const filtered = computed(() => {
  const term = search.value.trim().toLowerCase()
  return cycles.value.filter((c) => {
    if (botFilter.value !== 'all' && c.bot_id !== botFilter.value) return false
    if (symbolFilter.value !== 'all' && c.symbol !== symbolFilter.value) return false
    if (phaseFilter.value !== 'all' && c.phase !== phaseFilter.value) return false
    if (outcomeFilter.value !== 'all' && c.outcome !== outcomeFilter.value) return false
    if (term) {
      const hay = [c.trace_id, c.bot_id, c.symbol, c.outcome, c.phase, c.trigger_kind, c.signal, c.intent]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
      if (!hay.includes(term)) return false
    }
    return true
  })
})

const kpis = computed(() => {
  const total = cycles.value.length
  const accepted = cycles.value.filter((c) => c.outcome.startsWith('accepted')).length
  const rejected = cycles.value.filter((c) => c.outcome === 'risk_rejected' || c.risk_decision === 'rejected').length
  const latest = cycles.value[0]?.bar_timestamp ?? null
  return { total, accepted, rejected, latest }
})

function outcomeTone(outcome: CycleOutcome): 'green' | 'red' | 'yellow' | 'blue' | 'violet' | 'neutral' {
  switch (outcome) {
    case 'accepted_filled':
      return 'green'
    case 'accepted_partially_filled':
      return 'yellow'
    case 'accepted_no_fill':
      return 'blue'
    case 'risk_rejected':
      return 'red'
    case 'failed':
      return 'red'
    case 'no_op':
      return 'neutral'
  }
}

function outcomeLabel(outcome: CycleOutcome): string {
  return outcome.replace(/_/g, ' ')
}

function signalLabel(signal: string): string {
  return signal.replace(/_/g, ' ')
}

function intentLabel(intent: string): string {
  return intent.replace(/_/g, ' ')
}

function signalTone(signal: string): 'green' | 'red' | 'violet' | 'neutral' {
  if (signal.startsWith('buy')) return 'green'
  if (signal.startsWith('sell')) return 'red'
  if (signal === 'none' || signal === 'no_op') return 'neutral'
  return 'violet'
}

function intentTone(intent: string): 'green' | 'red' | 'blue' | 'neutral' {
  if (intent === 'open_long' || intent === 'add_long') return 'green'
  if (intent === 'reduce_long' || intent === 'close_long') return 'red'
  if (intent === 'no_op') return 'neutral'
  return 'blue'
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Cycles</span>
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
          eyebrow="Cycle traces"
          title="Follow every decision, end to end."
          description="From bar arrival through signal, intent, risk, and execution — every cycle the runtime recorded."
        />

        <section class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <MetricCard
            label="Loaded"
            :value="kpis.total"
            hint="Across visible bots"
          />
          <MetricCard
            label="Accepted"
            :value="kpis.accepted"
            accent="green"
            hint="Routed to execution"
          />
          <MetricCard
            label="Risk rejected"
            :value="kpis.rejected"
            :accent="kpis.rejected ? 'red' : 'neutral'"
            hint="Stopped by risk layer"
          />
          <MetricCard
            label="Newest bar"
            :value="kpis.latest ? fmtDate(kpis.latest, 'HH:mm') : '—'"
            :hint="kpis.latest ? fmtDate(kpis.latest, 'MMM d') : '—'"
          />
        </section>

        <section class="space-y-4">
          <div class="flex flex-wrap items-center gap-3">
            <UInput
              v-model="search"
              icon="i-lucide-search"
              placeholder="Search by trace, bot, symbol..."
              size="sm"
              class="flex-1 min-w-[240px]"
            />
            <USelect
              v-model="botFilter"
              :items="botItems"
              size="sm"
              class="w-48"
            />
            <USelect
              v-model="symbolFilter"
              :items="symbolItems"
              size="sm"
              class="w-40"
            />
            <USelect
              v-model="phaseFilter"
              :items="phaseItems"
              size="sm"
              class="w-40"
            />
            <USelect
              v-model="outcomeFilter"
              :items="outcomeItems"
              size="sm"
              class="w-52"
            />
            <div class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data ml-auto">
              {{ filtered.length }} / {{ cycles.length }}
            </div>
          </div>

          <EmptyState
            v-if="!filtered.length"
            icon="i-lucide-git-branch"
            title="No cycles match"
            description="Adjust filters or wait for the next polling cycle."
          />

          <div
            v-else
            class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3"
          >
            <NuxtLink
              v-for="(c, idx) in filtered.slice(0, 120)"
              :key="`${c.bot_id}-${c.trace_id ?? idx}`"
              :to="`/cycles/${encodeURIComponent(c.bot_id)}/${encodeURIComponent(c.trace_id)}`"
              class="surface-card p-4 hover:shadow-[0_2px_12px_rgba(0,0,0,0.04)] transition-all flex flex-col gap-3"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">
                    {{ c.bot_id }} · {{ c.phase }}
                  </div>
                  <div class="font-editorial text-[20px] leading-tight truncate">
                    {{ c.symbol }}
                  </div>
                  <div class="text-[11px] text-ink-soft font-data truncate">
                    {{ short(c.trace_id, 22) }}
                  </div>
                </div>
                <StatusPill
                  :label="outcomeLabel(c.outcome)"
                  :tone="outcomeTone(c.outcome)"
                />
              </div>

              <div class="grid grid-cols-3 gap-2 text-center">
                <div>
                  <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">Signal</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="signalLabel(c.signal as string)"
                      :tone="signalTone(c.signal as string)"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">Intent</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="intentLabel(c.intent as string)"
                      :tone="intentTone(c.intent as string)"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">Risk</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="c.risk_decision"
                      :tone="c.risk_decision === 'allowed' ? 'green' : 'red'"
                    />
                  </div>
                </div>
              </div>

              <div class="flex items-center justify-between text-[11px] text-ink-soft font-data">
                <span>{{ c.trigger_kind }}</span>
                <span>{{ fmtRelative(c.bar_timestamp) }}</span>
              </div>
            </NuxtLink>
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
