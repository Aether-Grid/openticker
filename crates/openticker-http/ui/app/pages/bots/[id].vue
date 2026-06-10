<script setup lang="ts">
import type { ActivityRecord, BotSummary } from '~/types/api'
import { fmtDate, fmtNumber, fmtPnL, fmtRelative, pnlColor, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const route = useRoute()
const botId = computed(() => route.params.id as string)

const { api } = useApi()
const actions = useServiceActions()
const snapshot = shallowRef<Record<string, unknown> | null>(null)
const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)

const load = async () => {
  pending.value = true
  try {
    snapshot.value = await api<Record<string, unknown>>(`/v1/bots/${botId.value}/snapshot`)
    lastLoadedAt.value = new Date()
  } catch {
    /* surfaced via empty state */
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
watch(
  botId,
  () => {
    void load()
  },
  { immediate: true }
)

const detail = computed<BotSummary | null>(() => (snapshot.value?.detail ?? snapshot.value) as BotSummary | null)
const lanes = computed<Array<Record<string, unknown>>>(
  () => ((detail.value as Record<string, unknown> | null)?.lanes as Array<Record<string, unknown>>) ?? []
)
const reconciliation = computed(() => snapshot.value?.reconciliation ?? snapshot.value?.report ?? null)
const timeline = computed<ActivityRecord[]>(
  () => (snapshot.value?.timeline as ActivityRecord[]) ?? (snapshot.value?.events as ActivityRecord[]) ?? []
)
const fills = computed<ActivityRecord[]>(() => (snapshot.value?.fills as ActivityRecord[]) ?? [])

const state = computed(() => detail.value?.state ?? '—')
const symbols = computed(() => (detail.value?.symbols ?? detail.value?.tickers ?? []) as string[])

const latestClose = computed<number | null>(() => {
  const lane = lanes.value.find((l) => typeof (l as Record<string, unknown>).last_close === 'number')
  if (lane) return (lane as Record<string, unknown>).last_close as number
  const latest = (snapshot.value as Record<string, unknown> | null)?.latest_bar as { close?: number } | undefined
  return typeof latest?.close === 'number' ? latest.close : null
})

const injectorOpen = ref(false)

async function act(fn: Promise<unknown>) {
  await fn
  await load()
}

function fillField(record: ActivityRecord, key: string): unknown {
  return (record as unknown as Record<string, unknown>)[key]
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
              to="/bots"
              class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data hover:text-ink transition-colors"
            >
              Bots /
            </NuxtLink>
            <span class="font-data text-[13px] text-ink">{{ botId }}</span>
          </div>
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
          :eyebrow="detail?.account ?? 'Instance'"
          :title="detail?.display_name ?? botId"
          :description="
            symbols.length
              ? `Trading ${symbols.join(' · ')} on ${detail?.venue ?? 'unknown venue'}`
              : 'Awaiting first poll.'
          "
        >
          <template #actions>
            <UButton
              :to="`/config/bots/${botId}`"
              color="neutral"
              variant="ghost"
              size="sm"
              icon="i-lucide-pencil"
              label="Edit config"
            />
            <UButton
              color="neutral"
              variant="solid"
              size="sm"
              icon="i-lucide-zap"
              label="Insert signal"
              @click="injectorOpen = true"
            />
            <BotStateBadge :state="state" />
          </template>
        </PageHeader>

        <section class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-3">
          <MetricCard
            label="Total PnL"
            :value="fmtPnL(detail?.pnl_total ?? 0)"
            :accent="(detail?.pnl_total ?? 0) >= 0 ? 'green' : 'red'"
            hint="Realized + unrealized"
          />
          <MetricCard
            label="Realized"
            :value="fmtPnL(detail?.pnl_realized ?? 0)"
            hint="Closed trades"
          />
          <MetricCard
            label="Unrealized"
            :value="fmtPnL(detail?.pnl_unrealized ?? 0)"
            hint="Open positions"
          />
          <MetricCard
            label="Position"
            :value="fmtNumber(detail?.position_quantity ?? 0, 4)"
            :hint="`Value ${fmtNumber(detail?.position_value ?? 0, 2)}`"
          />
          <MetricCard
            label="Last bar"
            :value="fmtRelative(detail?.polling?.last_bar_timestamp)"
            :hint="detail?.polling?.last_error ?? 'ok'"
          />
          <MetricCard
            label="Readiness"
            :value="detail?.readiness ?? (detail?.enabled === false ? 'disabled' : 'ready')"
            :hint="detail?.warmup ?? '—'"
          />
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-8 space-y-4">
            <SectionHeader
              label="Control desk"
              hint="Manual interventions. Kill switch still gates live execution."
            />
            <div class="surface-card p-5 space-y-2">
              <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-play"
                  label="Start"
                  block
                  @click="act(actions.startBot(botId))"
                />
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-power"
                  label="Stop"
                  block
                  @click="act(actions.stopBot(botId))"
                />
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-pause"
                  label="Pause"
                  block
                  @click="act(actions.pauseBot(botId))"
                />
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-circle-play"
                  label="Resume"
                  block
                  @click="act(actions.resumeBot(botId))"
                />
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-activity"
                  label="Tick once"
                  block
                  @click="act(actions.tickBot(botId))"
                />
                <UButton
                  color="neutral"
                  variant="outline"
                  icon="i-lucide-refresh-ccw"
                  label="Reconcile"
                  block
                  @click="act(actions.reconcileBot(botId))"
                />
                <UButton
                  color="error"
                  variant="outline"
                  icon="i-lucide-ban"
                  label="Cancel orders"
                  block
                  @click="act(actions.cancelOrders(botId))"
                />
                <UButton
                  color="error"
                  variant="outline"
                  icon="i-lucide-square-x"
                  label="Close positions"
                  block
                  @click="act(actions.closePositions(botId))"
                />
              </div>
              <div class="pt-2 border-t border-[color:var(--color-hairline)] flex items-center gap-2">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Manual tools</div>
                <UButton
                  color="neutral"
                  variant="soft"
                  size="xs"
                  icon="i-lucide-zap"
                  label="Insert signal"
                  @click="injectorOpen = true"
                />
                <span class="ml-auto text-[11px] text-ink-soft font-data">
                  Bypasses indicator, keeps risk + execution intact.
                </span>
              </div>
            </div>

            <SectionHeader
              label="Lanes"
              hint="Per-symbol state inside this bot."
            />
            <EmptyState
              v-if="!lanes.length"
              icon="i-lucide-list"
              title="No lanes yet"
              description="Lanes appear once the bot is matched against configured symbols."
            />
            <div
              v-else
              class="surface-card divide-y divide-[color:var(--color-hairline)]"
            >
              <div
                v-for="(lane, idx) in lanes"
                :key="(lane.symbol as string) ?? idx"
                class="px-5 py-4 grid grid-cols-12 gap-3 items-center"
              >
                <div class="col-span-3">
                  <div class="font-editorial text-[18px] leading-tight">
                    {{ lane.symbol ?? '—' }}
                  </div>
                  <div class="text-[11px] text-ink-soft font-data">
                    {{ lane.timeframe ?? lane.interval ?? '—' }}
                  </div>
                </div>
                <div class="col-span-3 text-right">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Position</div>
                  <div class="font-data tabular-nums text-[14px]">
                    {{ fmtNumber((lane.position_quantity as number) ?? 0, 4) }}
                  </div>
                </div>
                <div class="col-span-3 text-right">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Entry</div>
                  <div class="font-data tabular-nums text-[14px]">
                    {{ fmtNumber((lane.entry_price as number) ?? 0, 4) }}
                  </div>
                </div>
                <div class="col-span-3 text-right">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">PnL</div>
                  <div
                    class="font-data tabular-nums text-[14px]"
                    :class="pnlColor((lane.pnl_total as number) ?? 0)"
                  >
                    {{ fmtPnL((lane.pnl_total as number) ?? 0) }}
                  </div>
                </div>
              </div>
            </div>

            <SectionHeader
              label="Recent fills"
              hint="Last 25 filled orders attributed to this bot."
            />
            <EmptyState
              v-if="!fills.length"
              icon="i-lucide-receipt"
              title="No fills yet"
              description="Once orders execute, they land here."
            />
            <div
              v-else
              class="surface-card divide-y divide-[color:var(--color-hairline)]"
            >
              <div
                v-for="(f, idx) in fills.slice(0, 25)"
                :key="(f.id as string) ?? idx"
                class="px-5 py-3 grid grid-cols-12 gap-3 items-center"
              >
                <div class="col-span-3 font-data text-[12.5px] truncate">
                  {{ f.symbol ?? '—' }}
                </div>
                <div class="col-span-2 text-[11.5px] tracking-[0.08em] uppercase font-data">
                  {{ fillField(f, 'side') ?? '—' }}
                </div>
                <div class="col-span-2 font-data tabular-nums text-[12.5px]">
                  {{ fmtNumber((fillField(f, 'quantity') as number) ?? 0, 4) }}
                </div>
                <div class="col-span-2 font-data tabular-nums text-[12.5px]">
                  {{ fmtNumber((fillField(f, 'price') as number) ?? 0, 4) }}
                </div>
                <div class="col-span-3 text-right text-[11px] text-ink-soft font-data">
                  {{ fmtRelative(f.timestamp) }}
                </div>
              </div>
            </div>
          </div>

          <div class="col-span-12 xl:col-span-4 space-y-4">
            <SectionHeader
              label="Reconciliation"
              hint="Local vs. exchange comparison."
            />
            <div class="surface-card p-5 space-y-3">
              <div class="flex items-center justify-between">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Safe to trade</div>
                <StatusPill
                  :label="detail?.reconciliation?.blocked ? 'Blocked' : 'Clear'"
                  :tone="detail?.reconciliation?.blocked ? 'red' : 'green'"
                />
              </div>
              <div class="flex items-center justify-between">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Status</div>
                <div class="text-[13px] font-data">
                  {{ detail?.reconciliation?.status ?? '—' }}
                </div>
              </div>
              <div class="flex items-center justify-between">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Last report</div>
                <div class="text-[12.5px] font-data">
                  {{ fmtDate((detail?.reconciliation?.last_report_at as string) ?? null) }}
                </div>
              </div>
            </div>

            <SectionHeader
              label="Timeline"
              hint="Polling, signals, intents, risk, execution."
            />
            <EmptyState
              v-if="!timeline.length"
              icon="i-lucide-waypoints"
              title="Timeline is quiet"
              description="Events will stream in as the bot processes bars."
            />
            <div
              v-else
              class="surface-card divide-y divide-[color:var(--color-hairline)] max-h-[560px] overflow-y-auto scroll-fade-mask"
            >
              <div
                v-for="(event, idx) in timeline.slice(0, 40)"
                :key="(event.id as string) ?? idx"
                class="px-5 py-3"
              >
                <div class="flex items-center gap-2">
                  <StatusPill
                    :label="event.kind ?? 'event'"
                    tone="neutral"
                  />
                  <div class="text-[11px] text-ink-soft font-data ml-auto">
                    {{ fmtRelative(event.timestamp) }}
                  </div>
                </div>
                <div class="text-[12.5px] mt-1 truncate">
                  {{ event.reason ?? short(JSON.stringify(event.payload ?? event)) }}
                </div>
              </div>
            </div>

            <SectionHeader
              label="Reconciliation report"
              hint="Most recent full comparison."
            />
            <JsonBlock
              v-if="reconciliation"
              :value="reconciliation"
              max-height="360px"
            />
            <EmptyState
              v-else
              icon="i-lucide-search"
              title="No report"
              description="Run reconcile to generate a fresh comparison."
            />
          </div>
        </section>
        <SignalInjectorModal
          v-model:open="injectorOpen"
          :bot-id="botId"
          :bot-label="detail?.display_name ?? botId"
          :symbols="symbols"
          :default-price="latestClose"
          @applied="load"
        />
      </div>
    </template>
  </UDashboardPanel>
</template>
