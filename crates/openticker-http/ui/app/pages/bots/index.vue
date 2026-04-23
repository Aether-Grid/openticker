<script setup lang="ts">
import type { DropdownMenuItem } from '@nuxt/ui'
import type { BotSummary } from '~/types/api'
import { fmtNumber, fmtPnL, fmtRelative, pnlColor } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const actions = useServiceActions()
const router = useRouter()

const bots = shallowRef<BotSummary[]>([])
const pending = ref(false)
const error = shallowRef<unknown>(null)
const lastLoadedAt = ref<Date | null>(null)

const search = ref('')
const stateFilter = ref('all')
const availabilityFilter = ref('all')

const load = async () => {
  pending.value = true
  try {
    bots.value = await api<BotSummary[]>('/v1/bots?limit=500')
    error.value = null
    lastLoadedAt.value = new Date()
  } catch (err) {
    error.value = err
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})

const stateItems = [
  { label: 'All states', value: 'all' },
  { label: 'Running', value: 'running' },
  { label: 'Paused', value: 'paused' },
  { label: 'Reconciling', value: 'reconciling' },
  { label: 'Stopped', value: 'stopped' },
  { label: 'Blocked', value: 'blocked' }
]
const availabilityItems = [
  { label: 'All availability', value: 'all' },
  { label: 'Enabled', value: 'enabled' },
  { label: 'Disabled', value: 'disabled' }
]

const filtered = computed(() => {
  const term = search.value.trim().toLowerCase()
  return bots.value.filter((bot) => {
    if (term) {
      const hay = [
        bot.id,
        bot.display_name,
        bot.account,
        bot.venue,
        bot.market,
        ...(bot.symbols ?? []),
        ...(bot.tickers ?? [])
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
      if (!hay.includes(term)) return false
    }
    const state = (bot.state ?? '').toLowerCase()
    if (stateFilter.value !== 'all' && !state.includes(stateFilter.value)) return false
    if (availabilityFilter.value === 'enabled' && bot.enabled === false) return false
    if (availabilityFilter.value === 'disabled' && bot.enabled !== false) return false
    return true
  })
})

const columns = [
  { key: 'bot', label: 'Bot' },
  { key: 'venue', label: 'Venue' },
  { key: 'state', label: 'State' },
  { key: 'position', label: 'Position', align: 'right' as const },
  { key: 'pnl', label: 'PnL', align: 'right' as const },
  { key: 'last_bar', label: 'Last bar', align: 'right' as const },
  { key: 'actions', label: '', align: 'right' as const, width: '80px' }
]

const injectorOpen = ref(false)
const injectorTarget = shallowRef<BotSummary | null>(null)

function openInjector(bot: BotSummary) {
  injectorTarget.value = bot
  injectorOpen.value = true
}

function rowActions(bot: BotSummary): DropdownMenuItem[][] {
  const state = (bot.state ?? '').toLowerCase()
  return [
    [
      {
        label: state.includes('run') ? 'Pause' : 'Start',
        icon: state.includes('run') ? 'i-lucide-pause' : 'i-lucide-play',
        onSelect: () => {
          void (state.includes('run') ? actions.pauseBot(bot.id).then(load) : actions.startBot(bot.id).then(load))
        }
      },
      {
        label: 'Tick once',
        icon: 'i-lucide-activity',
        onSelect: () => {
          void actions.tickBot(bot.id).then(load)
        }
      },
      {
        label: 'Insert signal...',
        icon: 'i-lucide-zap',
        onSelect: () => openInjector(bot)
      },
      {
        label: 'Reconcile',
        icon: 'i-lucide-refresh-ccw',
        onSelect: () => {
          void actions.reconcileBot(bot.id).then(load)
        }
      }
    ],
    [
      {
        label: 'Stop',
        icon: 'i-lucide-power',
        onSelect: () => {
          void actions.stopBot(bot.id).then(load)
        }
      },
      {
        label: 'Cancel orders',
        icon: 'i-lucide-ban',
        color: 'error' as const,
        onSelect: () => {
          void actions.cancelOrders(bot.id).then(load)
        }
      },
      {
        label: 'Close positions',
        icon: 'i-lucide-square-x',
        color: 'error' as const,
        onSelect: () => {
          void actions.closePositions(bot.id).then(load)
        }
      }
    ]
  ]
}

const runCountBy = (predicate: (b: BotSummary) => boolean) => bots.value.filter(predicate).length

async function bulkStart() {
  await Promise.all(
    bots.value.filter((b) => !(b.state ?? '').toLowerCase().includes('run')).map((b) => actions.startBot(b.id))
  )
  await load()
}
async function bulkStop() {
  await Promise.all(bots.value.map((b) => actions.stopBot(b.id)))
  await load()
}
async function bulkPause() {
  await Promise.all(
    bots.value.filter((b) => (b.state ?? '').toLowerCase().includes('run')).map((b) => actions.pauseBot(b.id))
  )
  await load()
}
async function bulkResume() {
  await Promise.all(
    bots.value.filter((b) => (b.state ?? '').toLowerCase().includes('paus')).map((b) => actions.resumeBot(b.id))
  )
  await load()
}
async function bulkReconcile() {
  await Promise.all(bots.value.map((b) => actions.reconcileBot(b.id)))
  await load()
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Bots</span>
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
          eyebrow="Bots"
          title="Every instance, at a glance."
          description="Filter, inspect, and steer your bot fleet. Drill in for tick-level controls."
        >
          <template #actions>
            <UDropdownMenu
              :items="[
                [
                  { label: 'Start all', icon: 'i-lucide-play', onSelect: bulkStart },
                  { label: 'Stop all', icon: 'i-lucide-power', onSelect: bulkStop }
                ],
                [
                  { label: 'Pause running', icon: 'i-lucide-pause', onSelect: bulkPause },
                  { label: 'Resume paused', icon: 'i-lucide-circle-play', onSelect: bulkResume }
                ],
                [{ label: 'Reconcile all', icon: 'i-lucide-refresh-ccw', onSelect: bulkReconcile }]
              ]"
            >
              <UButton
                color="neutral"
                variant="solid"
                size="sm"
                icon="i-lucide-zap"
                label="Bulk actions"
                trailing-icon="i-lucide-chevron-down"
              />
            </UDropdownMenu>
          </template>
        </PageHeader>

        <section class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <MetricCard
            label="Total"
            :value="bots.length"
            hint="All registered bots"
          />
          <MetricCard
            label="Running"
            :value="runCountBy((b) => (b.state ?? '').toLowerCase().includes('run'))"
            accent="green"
            hint="Actively trading"
          />
          <MetricCard
            label="Paused"
            :value="runCountBy((b) => (b.state ?? '').toLowerCase().includes('paus'))"
            accent="yellow"
            hint="Temporarily halted"
          />
          <MetricCard
            label="Reconciling"
            :value="runCountBy((b) => (b.state ?? '').toLowerCase().includes('reconcil'))"
            accent="blue"
            hint="Aligning with exchange"
          />
        </section>

        <section class="space-y-4">
          <div class="flex flex-wrap items-center gap-3">
            <UInput
              v-model="search"
              icon="i-lucide-search"
              placeholder="Search by id, ticker, venue..."
              size="sm"
              class="flex-1 min-w-[240px]"
            />
            <USelect
              v-model="stateFilter"
              :items="stateItems"
              size="sm"
              class="w-48"
            />
            <USelect
              v-model="availabilityFilter"
              :items="availabilityItems"
              size="sm"
              class="w-48"
            />
            <div class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data ml-auto">
              {{ filtered.length }} / {{ bots.length }}
            </div>
          </div>

          <DataTable
            :columns="columns"
            :rows="filtered"
            :row-key="(r: BotSummary) => r.id"
            @row-click="(row: BotSummary) => router.push(`/bots/${row.id}`)"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'bot'">
                <div class="min-w-0">
                  <div class="font-editorial text-[16px] leading-tight truncate">
                    {{ (row as BotSummary).display_name ?? (row as BotSummary).id }}
                  </div>
                  <div class="text-[11px] text-ink-soft font-data truncate">
                    {{ (row as BotSummary).account ?? '—' }} ·
                    {{ ((row as BotSummary).symbols ?? (row as BotSummary).tickers ?? []).join(' · ') || '—' }}
                  </div>
                </div>
              </template>
              <template v-else-if="column.key === 'venue'">
                <div class="text-[12.5px]">
                  {{ (row as BotSummary).venue ?? '—' }}
                </div>
                <div class="text-[11px] text-ink-soft font-data">
                  {{ (row as BotSummary).market ?? '—' }}
                </div>
              </template>
              <template v-else-if="column.key === 'state'">
                <BotStateBadge :state="(row as BotSummary).state" />
              </template>
              <template v-else-if="column.key === 'position'">
                <div class="font-data tabular-nums text-[13px]">
                  {{ fmtNumber((row as BotSummary).position_quantity ?? 0, 4) }}
                </div>
                <div class="text-[11px] text-ink-soft font-data">
                  {{ fmtNumber((row as BotSummary).position_value ?? 0, 2) }}
                </div>
              </template>
              <template v-else-if="column.key === 'pnl'">
                <div
                  class="font-data tabular-nums text-[13.5px]"
                  :class="pnlColor((row as BotSummary).pnl_total ?? 0)"
                >
                  {{ fmtPnL((row as BotSummary).pnl_total ?? 0) }}
                </div>
                <div class="text-[11px] text-ink-soft font-data">
                  R {{ fmtPnL((row as BotSummary).pnl_realized ?? 0) }}
                </div>
              </template>
              <template v-else-if="column.key === 'last_bar'">
                <div class="text-[12px] font-data">
                  {{ fmtRelative((row as BotSummary).polling?.last_bar_timestamp) }}
                </div>
                <div class="text-[11px] text-ink-soft font-data truncate">
                  {{ (row as BotSummary).polling?.last_error || 'ok' }}
                </div>
              </template>
              <template v-else-if="column.key === 'actions'">
                <UDropdownMenu
                  :items="rowActions(row as BotSummary)"
                  @click.stop
                >
                  <UButton
                    color="neutral"
                    variant="ghost"
                    size="xs"
                    icon="i-lucide-more-horizontal"
                    aria-label="Actions"
                    @click.stop
                  />
                </UDropdownMenu>
              </template>
            </template>
          </DataTable>
        </section>

        <SignalInjectorModal
          v-if="injectorTarget"
          v-model:open="injectorOpen"
          :bot-id="injectorTarget.id"
          :bot-label="injectorTarget.display_name ?? injectorTarget.id"
          :symbols="(injectorTarget.symbols ?? injectorTarget.tickers ?? []) as string[]"
          @applied="load"
        />
      </div>
    </template>
  </UDashboardPanel>
</template>
