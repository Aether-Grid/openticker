<script setup lang="ts">
import type {
  BotConfig,
  EffectiveAccountConfig,
  MarketType,
  RiskProfileConfig,
  SignalMode,
  Timeframe
} from '~/types/api'
import { validateBot } from '~/utils/configValidation'
import { fmtNumber } from '~/utils/format'

definePageMeta({ layout: 'default' })

const route = useRoute()
const id = computed(() => route.params.id as string)

const { api } = useApi()
const actions = useConfigActions()
const service = useServiceActions()

// Stashed at load time so the selects + override inheritance render without a
// second fetch on every status poll.
const accounts = ref<EffectiveAccountConfig[]>([])
const riskProfiles = ref<RiskProfileConfig[]>([])

const { form, notFound, reload } = useConfigEditor<BotConfig>({
  load: async () => {
    const eff = await actions.fetchEffective()
    accounts.value = eff.accounts ?? []
    riskProfiles.value = eff.risk_profiles ?? []
    // EffectiveConfig.instances is serialized as `bots`.
    return eff.bots.find((b) => b.id === id.value) ?? null
  },
  // useConfigForm.submit passes the generation observed at the last load. The
  // PUT sends the whole BotConfig; locked fields pass through unchanged.
  save: (draft, generation) => actions.saveBot(id.value, draft, generation),
  validate: validateBot,
  // Bot scope lets bot-scoped 409s (timeframe_changed_running,
  // symbols_changed_running) map to inline fields; others fall to the banner.
  scope: () => `bot:${id.value}`
})

// ---- Runtime run-state (drives structural-field locking) --------------------
// Read the bot snapshot the same way the runtime detail page does. We tolerate
// either a top-level `detail` envelope or a flat snapshot, and a state enum of
// stopped|running|paused|reconciling (serialized snake_case on the backend).
const runState = ref<string>('')

const loadRunState = async () => {
  try {
    const snap = await api<Record<string, unknown>>(`/v1/bots/${id.value}/snapshot`)
    const detail = (snap?.detail ?? snap) as { state?: string } | null
    runState.value = detail?.state ?? ''
  } catch {
    // No snapshot (e.g. bot not loaded into the runtime) -> treat as not running
    // so the user can still edit. The server 409 stays authoritative on save.
    runState.value = ''
  }
}

// `running` (and reconciling) lock the structural fields. Tolerant substring
// match against the state string so an unexpected enum variant fails open to
// "not running" rather than wrongly locking everything.
const running = computed(() => {
  const s = runState.value.toLowerCase()
  return s.includes('run') || s.includes('reconcil')
})

onMounted(loadRunState)
watch(id, loadRunState)
useAutoRefresh(loadRunState)

const stopping = ref(false)
async function stopBot() {
  stopping.value = true
  try {
    await service.stopBot(id.value)
    await loadRunState()
  } finally {
    stopping.value = false
  }
}

/** "Stop bot and retry" from the conflict banner: stop, refresh state, re-save. */
async function stopAndRetry() {
  await stopBot()
  await form.submit()
}

// ---- Selects ----------------------------------------------------------------
const MARKET_ITEMS: { label: string; value: MarketType }[] = [
  { label: 'crypto', value: 'crypto' },
  { label: 'equities', value: 'equities' }
]
const TIMEFRAME_ITEMS: { label: string; value: Timeframe }[] = (
  ['1m', '5m', '15m', '30m', '1h', '4h', '1d'] as Timeframe[]
).map((tf) => ({ label: tf, value: tf }))
const SIGNAL_MODE_ITEMS: { label: string; value: SignalMode }[] = [
  { label: 'intrabar', value: 'intrabar' },
  { label: 'confirmed only', value: 'confirmed_only' }
]

const accountItems = computed(() => accounts.value.map((a) => ({ label: a.id, value: a.id })))
const riskProfileItems = computed(() => riskProfiles.value.map((p) => ({ label: p.id, value: p.id })))

// The account currently selected on the draft, used for the live-mode warning
// and the budget USD hint.
const selectedAccount = computed<EffectiveAccountConfig | null>(
  () => accounts.value.find((a) => a.id === form.draft.value?.account) ?? null
)

// Inherited risk values for the selected profile, shown as placeholders by
// ConfigRiskFields when an override is cleared. Recomputes when risk.profile
// changes.
const inheritedRisk = computed<Partial<RiskProfileConfig>>(
  () => riskProfiles.value.find((p) => p.id === form.draft.value?.risk.profile) ?? {}
)

// Budget USD hint: pct of the selected account's total budget.
const budgetUsdHint = computed<string | null>(() => {
  const acct = selectedAccount.value
  const pct = form.draft.value?.budget.pct
  if (!acct || typeof pct !== 'number' || !Number.isFinite(pct)) return null
  return `≈ ${fmtNumber((pct / 100) * acct.total_budget_usd, 2)} USD of ${acct.id}`
})

const LOCK_REASON = 'Stop the bot to change this'

// execution_constraints proxies: an empty input clears the field to null.
function setConstraint(key: 'quantity_step' | 'min_quantity' | 'min_notional_usd', value: number | undefined) {
  if (!form.draft.value) return
  form.draft.value.execution_constraints = {
    ...form.draft.value.execution_constraints,
    [key]: value === undefined ? null : value
  }
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
          <div class="flex items-center gap-2">
            <NuxtLink
              to="/config"
              class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data hover:text-ink transition-colors"
            >
              Config
            </NuxtLink>
            <span class="text-[11px] text-ink-soft font-data">/</span>
            <span class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data">bots</span>
            <span class="text-[11px] text-ink-soft font-data">/</span>
            <span class="font-data text-[13px] text-ink">{{ id }}</span>
          </div>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1100px] mx-auto w-full">
        <EmptyState
          v-if="notFound"
          icon="i-lucide-search-x"
          title="Bot not found in config"
          :description="`No bot with id '${id}' is present in the effective configuration.`"
        >
          <UButton
            to="/config"
            color="neutral"
            variant="outline"
            size="sm"
            icon="i-lucide-arrow-left"
            label="Back to config"
          />
        </EmptyState>

        <template v-else>
          <PageHeader
            eyebrow="Bot config"
            :title="id"
            description="Structural fields lock while the bot is running. Budget, polling, risk overrides and toggles apply on save."
          >
            <template #actions>
              <div class="flex items-center gap-3">
                <UButton
                  :to="`/bots/${id}`"
                  color="neutral"
                  variant="ghost"
                  size="sm"
                  icon="i-lucide-activity"
                  label="Runtime"
                />
                <BotStateBadge :state="runState || 'unknown'" />
              </div>
            </template>
          </PageHeader>

          <div
            v-if="running"
            class="mb-6"
          >
            <UAlert
              color="warning"
              variant="subtle"
              icon="i-lucide-lock"
              title="Bot is running — structural fields are locked"
              description="Market, symbols, timeframe, connectors, strategy, signal mode, indicators and warmup are read-only while running. Stop the bot to edit them."
              :ui="{ root: 'rounded-lg' }"
            >
              <template #actions>
                <UButton
                  color="warning"
                  variant="solid"
                  size="xs"
                  icon="i-lucide-power"
                  label="Stop bot"
                  :loading="stopping"
                  @click="stopBot"
                />
              </template>
            </UAlert>
          </div>

          <ConfigFormShell
            :dirty="form.dirty.value"
            :saving="form.saving.value"
            :stale="form.stale.value"
            :conflict="form.conflict.value"
            :general-error="form.generalError.value"
            :unmapped-violations="form.unmappedViolations.value"
            :pending="form.pending.value"
            @save="form.submit"
            @discard="form.discard"
            @reload="reload"
          >
            <template #conflict-actions>
              <UButton
                v-if="running"
                color="error"
                variant="solid"
                size="xs"
                icon="i-lucide-power"
                label="Stop bot and retry"
                :loading="stopping || form.saving.value"
                @click="stopAndRetry"
              />
            </template>

            <div
              v-if="form.draft.value"
              class="space-y-8"
            >
              <!-- Identity & structure (locked while running) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Identity & structure"
                  hint="Defines what the bot trades and how. Locked while the bot is running."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                  <ConfigField
                    label="Bot id"
                    :locked="true"
                    lock-reason="Bot id is immutable"
                    :error="form.errorFor('id')"
                  >
                    <UInput
                      :model-value="form.draft.value.id"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Market"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('market')"
                  >
                    <USelect
                      v-model="form.draft.value.market"
                      :items="MARKET_ITEMS"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Timeframe"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('timeframe')"
                  >
                    <USelect
                      v-model="form.draft.value.timeframe"
                      :items="TIMEFRAME_ITEMS"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>

                <ConfigField
                  label="Symbols"
                  hint="Press Enter to add"
                  :locked="running"
                  :lock-reason="LOCK_REASON"
                  :error="form.errorFor('symbols')"
                >
                  <ConfigStringList
                    v-model="form.draft.value.symbols"
                    :uppercase="true"
                    placeholder="BTC-USDT, ETH-USDT…"
                  />
                </ConfigField>

                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                  <ConfigField
                    label="Account"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('account')"
                  >
                    <USelect
                      v-model="form.draft.value.account"
                      :items="accountItems"
                      placeholder="Select account"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Strategy"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('strategy')"
                  >
                    <UInput
                      v-model="form.draft.value.strategy"
                      placeholder="e.g. trend_follow"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Signal mode"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('signal_mode')"
                  >
                    <USelect
                      v-model="form.draft.value.signal_mode"
                      :items="SIGNAL_MODE_ITEMS"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Data connector"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('data_connector')"
                  >
                    <UInput
                      v-model="form.draft.value.data_connector"
                      placeholder="e.g. binance"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Execution connector"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('execution_connector')"
                  >
                    <UInput
                      v-model="form.draft.value.execution_connector"
                      placeholder="e.g. binance"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Warmup target bars"
                    hint="Optional"
                    :locked="running"
                    :lock-reason="LOCK_REASON"
                    :error="form.errorFor('warmup_target_bars')"
                  >
                    <UInputNumber
                      :model-value="form.draft.value.warmup_target_bars ?? undefined"
                      :min="1"
                      :step="1"
                      placeholder="—"
                      class="font-data w-full"
                      @update:model-value="
                        (v: number | undefined) => {
                          if (form.draft.value) form.draft.value.warmup_target_bars = v === undefined ? null : v
                        }
                      "
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Indicators (locked while running) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Indicators"
                  hint="The indicator stack feeding the strategy. Locked while the bot is running."
                />
                <ConfigIndicatorList
                  v-model="form.draft.value.indicators"
                  :errors="form.errorFor"
                  :disabled="running"
                />
              </section>

              <!-- Runtime toggles (always editable) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Runtime"
                  hint="Applied on save without stopping the bot."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                  <ConfigField label="Enabled">
                    <USwitch v-model="form.draft.value.enabled" />
                  </ConfigField>
                  <ConfigField label="Polling enabled">
                    <USwitch v-model="form.draft.value.polling_enabled" />
                  </ConfigField>
                  <ConfigField
                    label="Polling interval ms"
                    :error="form.errorFor('polling_interval_ms')"
                  >
                    <UInputNumber
                      v-model="form.draft.value.polling_interval_ms"
                      :min="100"
                      :step="100"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Budget %"
                    :hint="budgetUsdHint ?? '0–100'"
                    :error="form.errorFor('budget.pct')"
                  >
                    <UInputNumber
                      v-model="form.draft.value.budget.pct"
                      :min="0"
                      :max="100"
                      :step="1"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <ConfigField label="Allow live trading">
                    <USwitch v-model="form.draft.value.allow_live" />
                    <p
                      v-if="form.draft.value.allow_live && selectedAccount?.mode === 'live'"
                      class="mt-2 text-[11.5px] text-[color:var(--color-pastel-red-ink)]"
                    >
                      Account <span class="font-data">{{ selectedAccount.id }}</span> is in live mode — this bot may
                      place real orders.
                    </p>
                  </ConfigField>
                </div>
              </section>

              <!-- Execution constraints (always editable) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Execution constraints"
                  hint="Optional venue rounding/minimums. Leave empty to inherit connector defaults."
                />
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                  <ConfigField
                    label="Quantity step"
                    :error="form.errorFor('execution_constraints.quantity_step')"
                  >
                    <UInputNumber
                      :model-value="form.draft.value.execution_constraints.quantity_step ?? undefined"
                      :min="0"
                      placeholder="—"
                      class="font-data w-full"
                      @update:model-value="(v: number | undefined) => setConstraint('quantity_step', v)"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Min quantity"
                    :error="form.errorFor('execution_constraints.min_quantity')"
                  >
                    <UInputNumber
                      :model-value="form.draft.value.execution_constraints.min_quantity ?? undefined"
                      :min="0"
                      placeholder="—"
                      class="font-data w-full"
                      @update:model-value="(v: number | undefined) => setConstraint('min_quantity', v)"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Min notional USD"
                    :error="form.errorFor('execution_constraints.min_notional_usd')"
                  >
                    <UInputNumber
                      :model-value="form.draft.value.execution_constraints.min_notional_usd ?? undefined"
                      :min="0"
                      placeholder="—"
                      class="font-data w-full"
                      @update:model-value="(v: number | undefined) => setConstraint('min_notional_usd', v)"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Risk (always editable) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Risk"
                  hint="The assigned profile sets the envelope; overrides below replace specific limits for this bot only."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                  <ConfigField
                    label="Risk profile"
                    :error="form.errorFor('risk.profile')"
                  >
                    <USelect
                      v-model="form.draft.value.risk.profile"
                      :items="riskProfileItems"
                      placeholder="Select profile"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
                <div class="space-y-2 pt-1">
                  <SectionHeader
                    label="Overrides"
                    hint="Leave a field empty to inherit the profile value (shown as a placeholder)."
                  />
                  <ConfigRiskFields
                    v-model="form.draft.value.risk.overrides"
                    :nullable="true"
                    path-prefix="risk.overrides"
                    :inherited="inheritedRisk"
                    :errors="form.errorFor"
                  />
                </div>
              </section>
            </div>
          </ConfigFormShell>
        </template>
      </div>
    </template>
  </UDashboardPanel>
</template>
