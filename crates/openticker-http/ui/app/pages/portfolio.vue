<script setup lang="ts">
import type { LedgerPortfolio } from '~/types/api'
import { fmtNumber, fmtPnL, fmtUsd, pnlColor } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const portfolio = shallowRef<LedgerPortfolio | null>(null)
const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)

const load = async () => {
  pending.value = true
  try {
    portfolio.value = await api<LedgerPortfolio>('/v1/ledger')
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})

const accounts = computed(() => portfolio.value?.accounts ?? [])
const bots = computed(() => portfolio.value?.bots ?? [])
const lanes = computed(() => portfolio.value?.lanes ?? [])

const summary = computed(() => {
  const cap = accounts.value.reduce((acc, a) => acc + (a.effective_cap_usd ?? 0), 0)
  const committed = accounts.value.reduce((acc, a) => acc + (a.total_committed_notional_usd ?? 0), 0)
  const tradeable = accounts.value.reduce((acc, a) => acc + (a.tradeable_open_room_usd ?? 0), 0)
  const blocked = accounts.value.reduce((acc, a) => acc + (a.blocked_open_room_usd ?? 0), 0)
  const unattrib = accounts.value.reduce((acc, a) => acc + (a.unattributed_open_notional_usd ?? 0), 0)
  return { cap, committed, tradeable, blocked, unattrib }
})

const accountColumns = [
  { key: 'id', label: 'Account' },
  { key: 'mode', label: 'Mode' },
  { key: 'declared', label: 'Declared', align: 'right' as const },
  { key: 'balance', label: 'Live balance', align: 'right' as const },
  { key: 'cap', label: 'Effective cap', align: 'right' as const },
  { key: 'committed', label: 'Committed', align: 'right' as const },
  { key: 'tradeable', label: 'Tradeable', align: 'right' as const }
]
const botColumns = [
  { key: 'id', label: 'Bot' },
  { key: 'account', label: 'Account' },
  { key: 'allocation', label: 'Allocation', align: 'right' as const },
  { key: 'committed', label: 'Committed', align: 'right' as const },
  { key: 'open', label: 'Open notional', align: 'right' as const },
  { key: 'reserved', label: 'Reserved', align: 'right' as const }
]
const laneColumns = [
  { key: 'path', label: 'Path' },
  { key: 'symbol', label: 'Symbol' },
  { key: 'position', label: 'Position', align: 'right' as const },
  { key: 'entry', label: 'Entry', align: 'right' as const },
  { key: 'notional', label: 'Notional', align: 'right' as const }
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Portfolio</span>
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
          eyebrow="Portfolio"
          title="Capital across accounts, bots, and lanes."
          description="Where capital sits, what's committed, and what's tradeable right now."
        />

        <section class="grid grid-cols-2 md:grid-cols-5 gap-3">
          <MetricCard
            label="Effective cap"
            :value="fmtUsd(summary.cap, true)"
            hint="Across accounts"
          />
          <MetricCard
            label="Committed"
            :value="fmtUsd(summary.committed, true)"
            accent="blue"
            hint="In live positions"
          />
          <MetricCard
            label="Tradeable"
            :value="fmtUsd(summary.tradeable, true)"
            accent="green"
            hint="Available room"
          />
          <MetricCard
            label="Blocked"
            :value="fmtUsd(summary.blocked, true)"
            :accent="summary.blocked ? 'red' : 'neutral'"
            hint="Reserved by guards"
          />
          <MetricCard
            label="Unattributed"
            :value="fmtUsd(summary.unattrib, true)"
            :accent="summary.unattrib ? 'yellow' : 'neutral'"
            hint="Not yet matched"
          />
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Account ledger"
            hint="Per-account capacity and commitments."
          />
          <DataTable
            :columns="accountColumns"
            :rows="accounts"
            :row-key="(r) => r.id"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'id'">
                <div class="font-editorial text-[15px] leading-tight">
                  {{ row.id }}
                </div>
                <div class="text-[11px] text-ink-soft font-data">
                  {{ row.kind ?? '—' }}
                </div>
              </template>
              <template v-else-if="column.key === 'mode'">
                <StatusPill
                  :label="row.mode ?? 'unknown'"
                  :tone="(row.mode ?? '').toLowerCase() === 'live' ? 'green' : 'blue'"
                />
              </template>
              <template v-else-if="column.key === 'declared'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.declared_cap_usd) }}</span>
              </template>
              <template v-else-if="column.key === 'balance'">
                <span class="font-data tabular-nums">{{
                  row.live_balance_usd === null || row.live_balance_usd === undefined
                    ? '—'
                    : fmtUsd(row.live_balance_usd)
                }}</span>
              </template>
              <template v-else-if="column.key === 'cap'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.effective_cap_usd) }}</span>
              </template>
              <template v-else-if="column.key === 'committed'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.total_committed_notional_usd) }}</span>
              </template>
              <template v-else-if="column.key === 'tradeable'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.tradeable_open_room_usd) }}</span>
              </template>
            </template>
          </DataTable>
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Bot allocations"
            hint="Slice of account capital routed to each bot."
          />
          <DataTable
            :columns="botColumns"
            :rows="bots"
            :row-key="(r) => `${r.account}-${r.id}`"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'id'">
                <NuxtLink
                  :to="`/bots/${row.id}`"
                  class="font-editorial text-[15px] hover:underline"
                >
                  {{ row.id }}
                </NuxtLink>
              </template>
              <template v-else-if="column.key === 'account'">
                <span class="font-data text-[12.5px]">{{ row.account ?? '—' }}</span>
              </template>
              <template v-else-if="column.key === 'allocation'">
                <span class="font-data tabular-nums">{{
                  row.allocation_percent ? `${fmtNumber(row.allocation_percent, 2)}%` : '—'
                }}</span>
              </template>
              <template v-else-if="column.key === 'committed'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.committed_notional_usd) }}</span>
              </template>
              <template v-else-if="column.key === 'open'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.open_notional_usd) }}</span>
              </template>
              <template v-else-if="column.key === 'reserved'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.reserved_notional_usd) }}</span>
              </template>
            </template>
          </DataTable>
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Lanes"
            hint="Open position per symbol, per bot."
          />
          <DataTable
            :columns="laneColumns"
            :rows="lanes"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'path'">
                <div class="text-[12.5px] font-data">{{ row.account }} → {{ row.bot }}</div>
              </template>
              <template v-else-if="column.key === 'symbol'">
                <span class="font-editorial text-[15px]">{{ row.symbol }}</span>
              </template>
              <template v-else-if="column.key === 'position'">
                <span
                  class="font-data tabular-nums"
                  :class="pnlColor(row.position_quantity ?? 0)"
                  >{{ fmtPnL(row.position_quantity ?? 0) }}</span
                >
              </template>
              <template v-else-if="column.key === 'entry'">
                <span class="font-data tabular-nums">{{ fmtNumber(row.entry_price ?? 0, 4) }}</span>
              </template>
              <template v-else-if="column.key === 'notional'">
                <span class="font-data tabular-nums">{{ fmtUsd(row.open_notional_usd) }}</span>
              </template>
            </template>
          </DataTable>
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Raw ledger"
            hint="Full JSON snapshot."
          />
          <JsonBlock
            :value="portfolio ?? {}"
            max-height="420px"
          />
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
