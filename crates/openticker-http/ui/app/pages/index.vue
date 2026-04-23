<script setup lang="ts">
import type { ActivityRecord, BotSummary, ConnectorStatus, StreamStatus } from '~/types/api'
import {
  extractItems,
  fmtCompact,
  fmtDate,
  fmtNumber,
  fmtPnL,
  fmtRelative,
  freshnessLabel,
  pnlColor,
  short,
  streamFreshness
} from '~/utils/format'

definePageMeta({ layout: 'default' })

const { snapshot, pending, lastLoadedAt, refresh } = useDashboardSnapshot(60)

const status = computed(() => snapshot.value?.status ?? null)
const bots = computed<BotSummary[]>(() => snapshot.value?.instances ?? [])
const streams = computed<StreamStatus[]>(() => snapshot.value?.dataStreams ?? [])
const connectors = computed<ConnectorStatus[]>(() => snapshot.value?.connectorsStatus ?? [])

const fills = computed<ActivityRecord[]>(() => extractItems(snapshot.value?.fills))
const orders = computed<ActivityRecord[]>(() => extractItems(snapshot.value?.orders))
const events = computed<ActivityRecord[]>(() => extractItems(snapshot.value?.events))
const signals = computed<ActivityRecord[]>(() => extractItems(snapshot.value?.signals))
const risk = computed<ActivityRecord[]>(() => extractItems(snapshot.value?.risk))

const runningCount = computed(() => {
  const explicit = status.value?.running_instances
  if (typeof explicit === 'number') return explicit
  return bots.value.filter((b) => (b.state ?? '').toLowerCase().includes('run')).length
})
const pausedCount = computed(() => {
  const explicit = status.value?.paused_instances
  if (typeof explicit === 'number') return explicit
  return bots.value.filter((b) => (b.state ?? '').toLowerCase().includes('paus')).length
})
const stoppedCount = computed(() => {
  const explicit = status.value?.stopped_instances
  if (typeof explicit === 'number') return explicit
  return bots.value.filter((b) => (b.state ?? '').toLowerCase().includes('stop')).length
})
const reconBlocked = computed(() => bots.value.filter((b) => b.reconciliation?.blocked).length)

const totalPnl = computed(() => bots.value.reduce((acc, b) => acc + (b.pnl_total ?? 0), 0))
const realizedPnl = computed(() => bots.value.reduce((acc, b) => acc + (b.pnl_realized ?? 0), 0))
const openNotional = computed(() => bots.value.reduce((acc, b) => acc + (b.position_value ?? 0), 0))

const topBots = computed(() =>
  [...bots.value].sort((a, b) => Math.abs(b.pnl_total ?? 0) - Math.abs(a.pnl_total ?? 0)).slice(0, 6)
)

const healthyStreams = computed(
  () => streams.value.filter((s) => streamFreshness(s.staleness_ms, s.last_error) === 'ok').length
)
const staleStreams = computed(() => streams.value.length - healthyStreams.value)
const connectorsUp = computed(
  () => connectors.value.filter((c) => (c.state ?? '').toLowerCase().includes('connect')).length
)

const activityFeed = computed<Array<ActivityRecord & { kind: string }>>(() => {
  const tagged = [
    ...fills.value.map((r) => ({ ...r, kind: 'fill' })),
    ...orders.value.map((r) => ({ ...r, kind: 'order' })),
    ...risk.value.map((r) => ({ ...r, kind: 'risk' })),
    ...signals.value.map((r) => ({ ...r, kind: 'signal' })),
    ...events.value.map((r) => ({ ...r, kind: r.kind ?? 'event' }))
  ]
  return tagged
    .filter((r) => r.timestamp)
    .sort((a, b) => (b.timestamp ?? '').localeCompare(a.timestamp ?? ''))
    .slice(0, 12)
})

function sparklineFor(stream: StreamStatus): number[] {
  if (Array.isArray(stream.sparkline)) return stream.sparkline.filter((v) => typeof v === 'number') as number[]
  return []
}

function kindTone(kind?: string): 'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'neutral' {
  const k = (kind ?? '').toLowerCase()
  if (k.includes('fill')) return 'green'
  if (k.includes('order')) return 'blue'
  if (k.includes('risk') || k.includes('reject')) return 'red'
  if (k.includes('signal')) return 'violet'
  if (k.includes('recon')) return 'yellow'
  return 'neutral'
}

function connectorTone(state?: string): 'green' | 'yellow' | 'red' | 'neutral' {
  const s = (state ?? '').toLowerCase()
  if (s.includes('connect')) return 'green'
  if (s.includes('degrad')) return 'yellow'
  if (s.includes('disconn') || s.includes('err')) return 'red'
  return 'neutral'
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Control Plane</span>
        </template>
        <template #right>
          <GlobalToolbar
            :status="status"
            :last-updated="lastLoadedAt"
            :pending="pending"
            @refresh="refresh"
          />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1400px] mx-auto w-full space-y-14">
        <PageHeader
          eyebrow="Overview"
          title="Your trading desk, calm and clear."
          description="A single-surface console for the runtime. Watch bots trade, keep capital healthy, and step in the moment something drifts."
        >
          <template #actions>
            <UButton
              color="neutral"
              variant="solid"
              size="sm"
              icon="i-lucide-bot"
              label="Manage bots"
              to="/bots"
            />
          </template>
        </PageHeader>

        <section class="grid grid-cols-12 gap-3">
          <div
            class="col-span-12 md:col-span-4 surface-card p-6 flex flex-col gap-4 min-h-[164px] relative overflow-hidden"
            style="background: linear-gradient(135deg, #ffffff 0%, var(--color-accent-softer) 100%)"
          >
            <div
              class="absolute -top-20 -right-20 w-48 h-48 rounded-full opacity-20 pointer-events-none"
              style="background: radial-gradient(circle, var(--color-accent) 0%, transparent 70%)"
            />
            <div class="flex items-start justify-between relative">
              <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Total PnL</div>
              <StatusPill
                :label="status?.mode_banner ?? 'Live'"
                tone="accent"
              />
            </div>
            <div class="flex items-end gap-6 justify-between">
              <div class="flex-1 min-w-0">
                <div
                  class="font-data tabular-nums text-[58px] leading-[0.9] truncate font-medium"
                  :class="pnlColor(totalPnl)"
                  style="letter-spacing: -0.035em"
                >
                  {{ fmtPnL(totalPnl) }}
                </div>
                <div class="mt-2 text-[12px] text-ink-soft">
                  Realized <span class="font-data tabular-nums text-ink">{{ fmtPnL(realizedPnl) }}</span>
                  <span class="mx-2">·</span>
                  Open notional <span class="font-data tabular-nums text-ink">{{ fmtCompact(openNotional) }}</span>
                </div>
              </div>
            </div>
          </div>

          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Running"
              :value="runningCount"
              :hint="bots.length ? `${bots.length} total bots` : 'No bots loaded'"
              accent="green"
            />
          </div>
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Paused"
              :value="pausedCount"
              :hint="pausedCount ? 'Resume from Bots' : 'None'"
              accent="yellow"
            />
          </div>
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Stopped"
              :value="stoppedCount"
              :hint="stoppedCount ? 'Disabled instances' : 'All enabled'"
              accent="neutral"
            />
          </div>
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Recon blocks"
              :value="reconBlocked"
              :hint="reconBlocked ? 'Needs attention' : 'Clear'"
              :accent="reconBlocked ? 'red' : 'neutral'"
            />
          </div>

          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Connectors up"
              :value="`${connectorsUp}/${connectors.length}`"
              :hint="connectors.length ? 'Live execution + data' : 'None registered'"
              accent="blue"
            />
          </div>
          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Data streams"
              :value="`${healthyStreams}/${streams.length}`"
              :hint="staleStreams ? `${staleStreams} stale` : 'All fresh'"
              :accent="staleStreams ? 'yellow' : 'green'"
            />
          </div>
          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Ready state"
              :value="status?.ready ? 'Ready' : 'Initializing'"
              :hint="`Instances · ${status?.total_instances ?? bots.length}`"
              :accent="status?.ready ? 'green' : 'yellow'"
            />
          </div>
          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Kill switch"
              :value="status?.kill_switch_active ? 'Engaged' : 'Off'"
              :hint="status?.kill_switch_active ? 'Trading blocked' : 'Normal'"
              :accent="status?.kill_switch_active ? 'red' : 'neutral'"
            />
          </div>
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-8 space-y-4">
            <SectionHeader
              label="Active bots"
              hint="Top performers and outliers, sorted by absolute PnL."
            >
              <template #actions>
                <UButton
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-arrow-right"
                  label="All bots"
                  to="/bots"
                />
              </template>
            </SectionHeader>

            <EmptyState
              v-if="!topBots.length"
              title="No bots loaded yet"
              description="Register a bot in your config, then reload to bring it online."
              icon="i-lucide-bot"
            />

            <div
              v-else
              class="grid grid-cols-1 md:grid-cols-2 gap-3"
            >
              <NuxtLink
                v-for="bot in topBots"
                :key="bot.id"
                :to="`/bots/${bot.id}`"
                class="surface-card p-5 hover:bg-[color:var(--color-accent-softer)] hover:border-[color:var(--color-accent)]/20 transition-all flex flex-col gap-3"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">
                      {{ bot.account ?? bot.venue ?? 'Instance' }}
                    </div>
                    <div class="font-editorial text-[22px] leading-tight truncate mt-0.5">
                      {{ bot.display_name ?? bot.id }}
                    </div>
                    <div class="mt-0.5 text-[12px] text-ink-soft font-data truncate">
                      {{ (bot.symbols ?? bot.tickers ?? []).join(' · ') || '—' }}
                    </div>
                  </div>
                  <BotStateBadge :state="bot.state" />
                </div>

                <div class="flex items-end justify-between gap-3 pt-1">
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">PnL</div>
                    <div
                      class="font-data text-[20px] tabular-nums"
                      :class="pnlColor(bot.pnl_total ?? 0)"
                    >
                      {{ fmtPnL(bot.pnl_total ?? 0) }}
                    </div>
                  </div>
                  <div class="text-right">
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Position</div>
                    <div class="font-data text-[14px] tabular-nums">
                      {{ fmtNumber(bot.position_quantity ?? 0, 4) }}
                    </div>
                    <div class="text-[11px] text-ink-soft">
                      {{ fmtRelative(bot.polling?.last_bar_timestamp) }}
                    </div>
                  </div>
                </div>
              </NuxtLink>
            </div>
          </div>

          <div class="col-span-12 xl:col-span-4 space-y-4">
            <SectionHeader
              label="Recent activity"
              hint="Fills, orders, signals, and risk events — live."
            >
              <template #actions>
                <UButton
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-arrow-right"
                  label="Open log"
                  to="/activity"
                />
              </template>
            </SectionHeader>

            <EmptyState
              v-if="!activityFeed.length"
              icon="i-lucide-activity"
              title="Quiet for now"
              description="No signals, orders, or fills yet."
            />

            <div
              v-else
              class="surface-card divide-y divide-[color:var(--color-hairline)] overflow-hidden"
            >
              <div
                v-for="(event, idx) in activityFeed"
                :key="(event.id as string) ?? `${event.kind}-${idx}`"
                class="px-5 py-3 flex items-start gap-3"
              >
                <StatusPill
                  :label="event.kind"
                  :tone="kindTone(event.kind)"
                />
                <div class="flex-1 min-w-0">
                  <div class="text-[12.5px] truncate">
                    <span class="font-medium">{{ event.bot_id ?? event.symbol ?? event.reason ?? 'event' }}</span>
                    <span
                      v-if="event.symbol && event.bot_id"
                      class="text-ink-soft"
                    >
                      · {{ event.symbol }}</span
                    >
                  </div>
                  <div class="text-[11px] text-ink-soft font-data mt-0.5">
                    {{ fmtRelative(event.timestamp) }}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-7 space-y-4">
            <SectionHeader
              label="Market data pulse"
              hint="Streams actively buffered in memory."
            >
              <template #actions>
                <UButton
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-arrow-right"
                  label="All feeds"
                  to="/feeds"
                />
              </template>
            </SectionHeader>

            <EmptyState
              v-if="!streams.length"
              icon="i-lucide-radio"
              title="No feeds active"
              description="Enable a market data connector to see streaming prices."
            />

            <div
              v-else
              class="grid grid-cols-1 md:grid-cols-2 gap-2.5"
            >
              <NuxtLink
                v-for="(stream, idx) in streams.slice(0, 8)"
                :key="`${stream.key.account_id}-${stream.key.symbol}-${stream.key.timeframe}-${idx}`"
                :to="`/feeds/${encodeURIComponent(stream.key.account_id)}/${encodeURIComponent(stream.key.symbol)}/${encodeURIComponent(stream.key.timeframe)}`"
                class="surface-card px-4 py-3 flex items-center gap-4 hover:bg-[color:var(--color-accent-softer)] transition-colors"
              >
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <div class="font-data text-[13px] text-ink font-medium tracking-tight truncate">
                      {{ stream.key.symbol }}
                    </div>
                    <span class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">{{
                      stream.key.timeframe
                    }}</span>
                  </div>
                  <div class="text-[11px] text-ink-soft mt-0.5 truncate">
                    {{ stream.key.account_id }}
                  </div>
                </div>
                <Sparkline
                  :values="sparklineFor(stream)"
                  :width="110"
                  :height="28"
                  area
                />
                <div class="text-right shrink-0">
                  <div class="font-data text-[13px] tabular-nums">
                    {{ fmtNumber(stream.latest_bar?.close ?? 0, 4) }}
                  </div>
                  <div class="text-[10.5px] text-ink-soft font-data uppercase tracking-[0.12em]">
                    {{ freshnessLabel(streamFreshness(stream.staleness_ms, stream.last_error)) }}
                  </div>
                </div>
              </NuxtLink>
            </div>
          </div>

          <div class="col-span-12 xl:col-span-5 space-y-4">
            <SectionHeader
              label="Connector matrix"
              hint="Execution & data connectors in the runtime."
            >
              <template #actions>
                <UButton
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-arrow-right"
                  label="Details"
                  to="/connectors"
                />
              </template>
            </SectionHeader>

            <EmptyState
              v-if="!connectors.length"
              icon="i-lucide-plug"
              title="No connectors registered"
              description="Hook up an exchange adapter to start trading."
            />

            <div
              v-else
              class="surface-card divide-y divide-[color:var(--color-hairline)]"
            >
              <div
                v-for="(c, idx) in connectors.slice(0, 10)"
                :key="`${c.account}-${idx}`"
                class="px-5 py-3 flex items-center gap-3"
              >
                <StatusPill
                  :label="c.state ?? 'unknown'"
                  :tone="connectorTone(c.state)"
                />
                <div class="flex-1 min-w-0">
                  <div class="text-[13px] font-medium truncate">
                    {{ c.account }}
                  </div>
                  <div class="text-[11px] text-ink-soft font-data truncate">
                    {{ c.kind ?? '—' }} · {{ c.mode ?? '—' }}
                  </div>
                </div>
                <div class="text-[11px] text-ink-soft font-data truncate max-w-[160px] text-right">
                  {{ short(c.message ?? '', 26) }}
                </div>
              </div>
            </div>
          </div>
        </section>

        <div class="pt-4 pb-6 text-[11px] text-ink-soft font-data text-center uppercase tracking-[0.18em]">
          Last update {{ fmtDate(lastLoadedAt?.toISOString()) }}
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
