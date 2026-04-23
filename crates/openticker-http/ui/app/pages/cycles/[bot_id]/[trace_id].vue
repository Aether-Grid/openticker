<script setup lang="ts">
import type { CycleOutcome, CycleSummary, CycleTrace } from '~/types/api'
import { fmtDateTime, fmtNumber, fmtPnL, fmtRelative, pnlColor, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const route = useRoute()
const botId = computed(() => route.params.bot_id as string)
const traceId = computed(() => route.params.trace_id as string)

const { api } = useApi()
const toast = useToast()

const cycle = shallowRef<CycleTrace | null>(null)
const siblings = shallowRef<CycleSummary[]>([])
const pending = ref(false)
const error = shallowRef<string | null>(null)
const lastLoadedAt = ref<Date | null>(null)

const load = async () => {
  pending.value = true
  error.value = null
  try {
    const [detail, list] = await Promise.all([
      api<CycleTrace>(`/v1/bots/${encodeURIComponent(botId.value)}/cycles/${encodeURIComponent(traceId.value)}`),
      api<CycleSummary[]>(`/v1/bots/${encodeURIComponent(botId.value)}/cycles?limit=200`).catch(
        () => [] as CycleSummary[]
      )
    ])
    cycle.value = detail
    siblings.value = list
    lastLoadedAt.value = new Date()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load cycle trace.'
    cycle.value = null
  } finally {
    pending.value = false
  }
}

watch(
  [botId, traceId],
  () => {
    void load()
  },
  { immediate: true }
)

const siblingIndex = computed(() => siblings.value.findIndex((c) => c.trace_id === traceId.value))
const prevCycle = computed(() => siblings.value[siblingIndex.value + 1])
const nextCycle = computed(() => siblings.value[siblingIndex.value - 1])

const summary = computed(() => cycle.value?.summary ?? null)
const trigger = computed(() => cycle.value?.trigger ?? null)
const signalStep = computed(() => cycle.value?.signal_step ?? null)
const intentStep = computed(() => cycle.value?.intent_step ?? null)
const riskStep = computed(() => cycle.value?.risk_step ?? null)
const executionStep = computed(() => cycle.value?.execution_step ?? null)
const positionStep = computed(() => cycle.value?.position_step ?? null)
const recon = computed(() => cycle.value?.reconciliation_context?.latest ?? null)
const relatedRecords = computed(() => cycle.value?.related_records ?? [])
const relatedEvents = computed(() => cycle.value?.related_events ?? [])

function labelize(value?: string | null): string {
  return value ? value.replace(/_/g, ' ') : '—'
}

function outcomeTone(outcome?: CycleOutcome): 'green' | 'red' | 'yellow' | 'blue' | 'neutral' {
  switch (outcome) {
    case 'accepted_filled':
      return 'green'
    case 'accepted_partially_filled':
      return 'yellow'
    case 'accepted_no_fill':
      return 'blue'
    case 'risk_rejected':
    case 'failed':
      return 'red'
    default:
      return 'neutral'
  }
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

const steps = computed(() => {
  const rows: Array<{
    label: string
    status: string
    tone: 'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'neutral'
    hint?: string
  }> = []

  if (trigger.value) {
    rows.push({
      label: 'Trigger',
      status: labelize(trigger.value.trigger_kind),
      tone: 'neutral',
      hint: `close ${fmtNumber(trigger.value.close, 4)}`
    })
  }
  if (signalStep.value) {
    rows.push({
      label: 'Signal',
      status: labelize(signalStep.value.signal as string),
      tone: signalTone(signalStep.value.signal as string),
      hint: `close ${fmtNumber(signalStep.value.close, 4)}`
    })
  }
  if (intentStep.value) {
    rows.push({
      label: 'Intent',
      status: labelize(intentStep.value.intent as string),
      tone: intentTone(intentStep.value.intent as string),
      hint: `qty ${fmtNumber(intentStep.value.order_quantity, 4)}`
    })
  }
  if (riskStep.value) {
    rows.push({
      label: 'Risk',
      status: labelize(riskStep.value.decision),
      tone: riskStep.value.decision === 'allowed' ? 'green' : 'red',
      hint: riskStep.value.reason ?? undefined
    })
  }
  if (executionStep.value) {
    const status = executionStep.value.fill
      ? 'filled'
      : executionStep.value.order
        ? executionStep.value.order.status
        : 'no execution'
    rows.push({
      label: 'Execution',
      status: labelize(status),
      tone: executionStep.value.fill ? 'green' : executionStep.value.order ? 'blue' : 'neutral'
    })
  }
  if (positionStep.value) {
    const after = positionStep.value.has_position_after
    rows.push({
      label: 'Position',
      status: after ? 'open' : 'flat',
      tone: after ? 'green' : 'neutral',
      hint: `qty ${fmtNumber(positionStep.value.quantity_after, 4)}`
    })
  }
  if (recon.value) {
    rows.push({
      label: 'Reconciliation',
      status: recon.value.safe_to_trade ? 'clear' : 'blocked',
      tone: recon.value.safe_to_trade ? 'green' : 'red',
      hint: recon.value.source
    })
  }
  return rows
})

async function copyTrace() {
  try {
    await navigator.clipboard.writeText(traceId.value)
    toast.add({ title: 'Trace ID copied', icon: 'i-lucide-check', color: 'success', duration: 1800 })
  } catch {
    /* noop */
  }
}

function relatedRecordSummary(record: Record<string, unknown>): string {
  const parts = [
    record.family as string,
    record.label as string,
    record.client_order_id as string,
    record.bar_timestamp as string
  ].filter(Boolean) as string[]
  return parts.join(' · ')
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
              to="/cycles"
              class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data hover:text-ink transition-colors"
            >
              Cycles /
            </NuxtLink>
            <span class="font-data text-[13px] truncate max-w-[360px]">{{ short(traceId, 32) }}</span>
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
          :eyebrow="summary ? `${summary.bot_id} · ${summary.symbol}` : botId"
          :title="summary ? `${labelize(summary.phase)} · ${labelize(summary.outcome)}` : 'Loading cycle…'"
          :description="
            summary
              ? `Bar ${fmtDateTime(summary.bar_timestamp)} · Recorded ${fmtDateTime(new Date(summary.created_at_ms).toISOString())}`
              : 'Fetching detail from journal…'
          "
        >
          <template #actions>
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-arrow-left"
              :disabled="!prevCycle"
              aria-label="Previous cycle"
              @click="
                prevCycle &&
                $router.push(`/cycles/${encodeURIComponent(botId)}/${encodeURIComponent(prevCycle.trace_id)}`)
              "
            />
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-arrow-right"
              :disabled="!nextCycle"
              aria-label="Next cycle"
              @click="
                nextCycle &&
                $router.push(`/cycles/${encodeURIComponent(botId)}/${encodeURIComponent(nextCycle.trace_id)}`)
              "
            />
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-copy"
              label="Copy trace"
              @click="copyTrace"
            />
          </template>
        </PageHeader>

        <div
          v-if="error"
          class="surface-card p-6 flex items-center gap-3 text-[13px] border-l-[3px] border-l-[color:var(--color-pastel-red-ink)]"
        >
          <UIcon
            name="i-lucide-triangle-alert"
            class="size-5 text-[color:var(--color-pastel-red-ink)]"
          />
          <div>
            <div class="font-medium">Could not load cycle trace.</div>
            <div class="text-ink-soft mt-0.5">{{ error }}</div>
          </div>
        </div>

        <section
          v-if="steps.length"
          class="surface-card p-5 overflow-x-auto"
        >
          <div class="flex items-center gap-2 min-w-[760px]">
            <template
              v-for="(step, idx) in steps"
              :key="step.label"
            >
              <div class="flex-1 min-w-[104px]">
                <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft font-data">
                  {{ step.label }}
                </div>
                <div class="mt-1.5">
                  <StatusPill
                    :label="short(step.status, 20)"
                    :tone="step.tone"
                  />
                </div>
                <div
                  v-if="step.hint"
                  class="mt-1.5 text-[11px] text-ink-soft font-data truncate"
                >
                  {{ step.hint }}
                </div>
              </div>
              <div
                v-if="idx < steps.length - 1"
                class="w-6 h-px bg-[color:var(--color-hairline)] shrink-0"
              />
            </template>
          </div>
        </section>

        <section
          v-if="summary"
          class="grid grid-cols-2 md:grid-cols-5 gap-3"
        >
          <MetricCard
            label="Outcome"
            :value="labelize(summary.outcome)"
            :accent="outcomeTone(summary.outcome)"
          />
          <MetricCard
            label="Signal"
            :value="labelize(summary.signal as string)"
            :accent="signalTone(summary.signal as string)"
          />
          <MetricCard
            label="Intent"
            :value="labelize(summary.intent as string)"
            :accent="intentTone(summary.intent as string)"
          />
          <MetricCard
            label="Risk"
            :value="summary.risk_decision"
            :accent="summary.risk_decision === 'allowed' ? 'green' : 'red'"
          />
          <MetricCard
            label="Phase"
            :value="summary.phase"
            :hint="summary.trigger_kind"
          />
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-6 space-y-6">
            <article
              v-if="trigger"
              class="surface-card p-6 space-y-4"
            >
              <SectionHeader
                label="Trigger"
                hint="What initiated this cycle."
              />
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Source</div>
                  <div class="mt-1 font-data">
                    {{ trigger.signal_source }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Kind</div>
                  <div class="mt-1 font-data">
                    {{ labelize(trigger.trigger_kind) }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Bar close</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(trigger.close, 4) }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Bar time</div>
                  <div class="mt-1 font-data">
                    {{ fmtDateTime(trigger.bar_timestamp) }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="signalStep"
              class="surface-card p-6 space-y-4 border-l-[3px] border-l-[color:var(--color-accent-strong)]"
            >
              <SectionHeader
                label="Signal"
                hint="Indicator output."
              >
                <template #actions>
                  <StatusPill
                    :label="labelize(signalStep.signal as string)"
                    :tone="signalTone(signalStep.signal as string)"
                  />
                </template>
              </SectionHeader>
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Close</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(signalStep.close, 4) }}
                  </div>
                </div>
                <div v-if="signalStep.metadata">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Metadata</div>
                  <div class="mt-1 text-[11.5px] text-ink-soft font-data truncate">
                    {{ short(JSON.stringify(signalStep.metadata), 64) }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="intentStep"
              class="surface-card p-6 space-y-4"
            >
              <SectionHeader
                label="Intent"
                hint="What the strategy wanted to do."
              >
                <template #actions>
                  <StatusPill
                    :label="labelize(intentStep.intent as string)"
                    :tone="intentTone(intentStep.intent as string)"
                  />
                </template>
              </SectionHeader>
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Order quantity</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(intentStep.order_quantity, 4) }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Had position</div>
                  <div class="mt-1 font-data">
                    {{ intentStep.has_position_before ? 'yes' : 'no' }}
                  </div>
                </div>
                <div
                  v-if="intentStep.strategy_rationale"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Rationale</div>
                  <div class="mt-1 text-[12.5px] leading-snug">
                    {{ intentStep.strategy_rationale }}
                  </div>
                </div>
                <div
                  v-if="intentStep.order_quantity_adjustment_reason"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Adjustment</div>
                  <div class="mt-1 text-[12.5px] text-ink-soft">
                    {{ intentStep.order_quantity_adjustment_reason }}
                  </div>
                </div>
                <div
                  v-if="intentStep.order_ledger_outcome"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Ledger outcome</div>
                  <div class="mt-1 text-[12.5px] text-ink-soft">
                    {{ intentStep.order_ledger_outcome }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="riskStep"
              class="surface-card p-6 space-y-4"
              :class="
                riskStep.decision === 'rejected'
                  ? 'border-l-[3px] border-l-[color:var(--color-pastel-red-ink)]'
                  : 'border-l-[3px] border-l-[color:var(--color-pastel-green-ink)]'
              "
            >
              <SectionHeader
                label="Risk decision"
                hint="Guards that ran against the intent."
              >
                <template #actions>
                  <StatusPill
                    :label="riskStep.decision"
                    :tone="riskStep.decision === 'allowed' ? 'green' : 'red'"
                  />
                </template>
              </SectionHeader>
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div
                  v-if="riskStep.reason"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Reason</div>
                  <div class="mt-1">
                    {{ riskStep.reason }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Stale data</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="riskStep.stale_data ? 'yes' : 'no'"
                      :tone="riskStep.stale_data ? 'yellow' : 'neutral'"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Cooldown</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="riskStep.cooldown_active ? 'active' : 'off'"
                      :tone="riskStep.cooldown_active ? 'yellow' : 'neutral'"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Open positions</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ riskStep.account_open_positions }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Daily loss %</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(riskStep.account_daily_loss_pct * 100, 2) }}%
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Spread bps</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ riskStep.observed_spread_bps }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Slippage est. bps</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ riskStep.estimated_slippage_bps }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Room · bot</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(riskStep.budget_room.remaining_bot_usd, 2) }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Room · account</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(riskStep.budget_room.remaining_account_usd, 2) }}
                  </div>
                </div>
              </div>
            </article>
          </div>

          <div class="col-span-12 xl:col-span-6 space-y-6">
            <article
              v-if="executionStep"
              class="surface-card p-6 space-y-4"
            >
              <SectionHeader
                label="Execution"
                hint="Router output and fills."
              />
              <div
                v-if="!executionStep.order && !executionStep.fill"
                class="text-[13px] text-ink-soft"
              >
                No order was submitted.
              </div>
              <div
                v-if="executionStep.order"
                class="space-y-3"
              >
                <div class="flex items-center justify-between">
                  <span class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Order</span>
                  <StatusPill
                    :label="executionStep.order.status"
                    :tone="executionStep.order.status.toLowerCase().includes('fill') ? 'green' : 'blue'"
                  />
                </div>
                <div class="grid grid-cols-3 gap-4 text-[12.5px]">
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Qty</div>
                    <div class="mt-1 font-data tabular-nums">
                      {{ fmtNumber(executionStep.order.quantity, 4) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Price</div>
                    <div class="mt-1 font-data tabular-nums">
                      {{ fmtNumber(executionStep.order.price, 4) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Intent</div>
                    <div class="mt-1 font-data">
                      {{ labelize(executionStep.order.intent as string) }}
                    </div>
                  </div>
                  <div class="col-span-3">
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Client order id</div>
                    <div class="mt-1 font-data text-[11.5px] truncate">
                      {{ executionStep.order.client_order_id }}
                    </div>
                  </div>
                </div>
              </div>
              <div
                v-if="executionStep.fill"
                class="space-y-3 pt-4 hairline-t"
              >
                <div class="flex items-center justify-between">
                  <span class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Fill</span>
                  <StatusPill
                    label="filled"
                    tone="green"
                  />
                </div>
                <div class="grid grid-cols-3 gap-4 text-[12.5px]">
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Qty</div>
                    <div class="mt-1 font-data tabular-nums">
                      {{ fmtNumber(executionStep.fill.quantity, 4) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Price</div>
                    <div class="mt-1 font-data tabular-nums">
                      {{ fmtNumber(executionStep.fill.price, 4) }}
                    </div>
                  </div>
                  <div>
                    <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Fee</div>
                    <div class="mt-1 font-data tabular-nums">
                      {{
                        executionStep.fill.fee_amount != null
                          ? `${fmtNumber(executionStep.fill.fee_amount, 4)} ${executionStep.fill.fee_asset ?? ''}`
                          : '—'
                      }}
                    </div>
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="positionStep"
              class="surface-card p-6 space-y-4"
            >
              <SectionHeader
                label="Position outcome"
                hint="Lane state after the cycle."
              />
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Before</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="positionStep.has_position_before ? 'open' : 'flat'"
                      :tone="positionStep.has_position_before ? 'blue' : 'neutral'"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">After</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="positionStep.has_position_after ? 'open' : 'flat'"
                      :tone="positionStep.has_position_after ? 'green' : 'neutral'"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Quantity</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(positionStep.quantity_after, 4) }}
                  </div>
                </div>
                <div v-if="positionStep.entry_price_after !== undefined">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Entry price</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ fmtNumber(positionStep.entry_price_after, 4) }}
                  </div>
                </div>
                <div class="col-span-2">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Realized PnL</div>
                  <div
                    class="mt-1 font-data tabular-nums text-[18px]"
                    :class="pnlColor(positionStep.realized_pnl_usd)"
                  >
                    {{ fmtPnL(positionStep.realized_pnl_usd) }}
                  </div>
                </div>
                <div
                  v-if="positionStep.reason"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Reason</div>
                  <div class="mt-1 text-ink-soft">
                    {{ positionStep.reason }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="recon"
              class="surface-card p-6 space-y-4"
              :class="recon.safe_to_trade ? '' : 'border-l-[3px] border-l-[color:var(--color-pastel-red-ink)]'"
            >
              <SectionHeader
                label="Reconciliation snapshot"
                hint="Local vs. exchange at decision time."
              >
                <template #actions>
                  <StatusPill
                    :label="recon.safe_to_trade ? 'clear' : 'blocked'"
                    :tone="recon.safe_to_trade ? 'green' : 'red'"
                  />
                </template>
              </SectionHeader>
              <div class="grid grid-cols-2 gap-4 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Source</div>
                  <div class="mt-1 font-data">
                    {{ recon.source }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Recorded</div>
                  <div class="mt-1 font-data">
                    {{ fmtRelative(new Date(recon.created_at_ms).toISOString()) }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Local open orders</div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ recon.local_open_orders }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">
                    Connector open orders
                  </div>
                  <div class="mt-1 font-data tabular-nums">
                    {{ recon.connector_open_orders }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Local position</div>
                  <div class="mt-1">
                    {{ recon.local_has_position ? 'yes' : 'no' }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">
                    Connector position
                  </div>
                  <div class="mt-1">
                    {{ recon.connector_has_position ? 'yes' : 'no' }}
                  </div>
                </div>
                <div
                  v-if="recon.reason"
                  class="col-span-2"
                >
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Reason</div>
                  <div class="mt-1 text-ink-soft">
                    {{ recon.reason }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="relatedRecords.length"
              class="surface-card p-6 space-y-3"
            >
              <SectionHeader
                label="Related records"
                :hint="`${relatedRecords.length} journal entries linked to this trace.`"
              />
              <div class="divide-y divide-[color:var(--color-hairline)]">
                <div
                  v-for="(rec, idx) in relatedRecords"
                  :key="idx"
                  class="py-2.5 text-[12.5px] flex items-center gap-3"
                >
                  <StatusPill
                    :label="(rec.family as string) ?? 'record'"
                    tone="neutral"
                  />
                  <div class="flex-1 min-w-0 text-ink-soft font-data truncate">
                    {{ relatedRecordSummary(rec) }}
                  </div>
                </div>
              </div>
            </article>

            <article
              v-if="relatedEvents.length"
              class="surface-card p-6 space-y-3"
            >
              <SectionHeader
                label="Related events"
                :hint="`${relatedEvents.length} runtime events.`"
              />
              <div class="divide-y divide-[color:var(--color-hairline)]">
                <div
                  v-for="(ev, idx) in relatedEvents"
                  :key="idx"
                  class="py-2.5 text-[12.5px] flex items-center gap-3"
                >
                  <StatusPill
                    :label="(ev.kind as string) ?? 'event'"
                    tone="violet"
                  />
                  <div class="flex-1 min-w-0 text-ink-soft font-data truncate">
                    {{ short(JSON.stringify(ev.payload ?? {}), 90) }}
                  </div>
                </div>
              </div>
            </article>
          </div>
        </section>

        <section class="space-y-4">
          <SectionHeader
            label="Raw cycle payload"
            hint="Full JSON for debugging and exports."
          />
          <JsonBlock
            :value="cycle ?? {}"
            max-height="520px"
          />
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
