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

const router = useRouter()
const actions = useConfigActions()
const status = useConfigStatus()

const accounts = ref<EffectiveAccountConfig[]>([])
const riskProfiles = ref<RiskProfileConfig[]>([])

// The in-progress bot id, mirrored out of the form so the `scope` getter can
// read it without referencing `form` inside its own initializer (which would
// make the form's type circular).
const draftId = ref('')

/** A sensible blank-slate bot, parameterised by the first available ids. */
function template(): BotConfig {
  const firstAccount = accounts.value[0]?.id ?? ''
  const firstProfile = riskProfiles.value[0]?.id ?? ''
  return {
    id: '',
    enabled: false,
    market: 'crypto',
    symbols: [],
    timeframe: '5m',
    account: firstAccount,
    data_connector: '',
    execution_connector: '',
    strategy: '',
    signal_mode: 'confirmed_only',
    polling_enabled: true,
    polling_interval_ms: 5000,
    indicators: [],
    execution_constraints: { quantity_step: null, min_quantity: null, min_notional_usd: null },
    budget: { pct: 0 },
    risk: { profile: firstProfile, overrides: {} },
    warmup_target_bars: null,
    allow_live: false
  }
}

// A create form has no existing entity, and a successful POST navigates away —
// so the reload-after-save in useConfigEditor would be wrong here. We drive
// useConfigForm directly: load() seeds the default template, save() POSTs, and
// the page navigates on `ok` instead of reloading.
const form = useConfigForm<BotConfig>({
  load: async () => {
    const eff = await actions.fetchEffective()
    accounts.value = eff.accounts ?? []
    riskProfiles.value = eff.risk_profiles ?? []
    return template()
  },
  save: async (draft, generation) => {
    const result = await actions.createBot(draft, generation)
    if (result.ok) {
      // Navigate to the new bot's editor; the create form is unmounted so its
      // own reload never runs against a now-stale empty template.
      await router.push(`/config/bots/${draft.id}`)
    }
    return result
  },
  // create:true also enforces the slug rule on the id (filename-safe).
  validate: (draft) => validateBot(draft, { create: true }),
  // Bot scope so a duplicate_bot 409 attributes correctly; the code has no field
  // mapping, so it surfaces in the conflict banner regardless. Reads `draftId`
  // (not `form`) to avoid a circular type reference inside this initializer.
  scope: (): string => `bot:${draftId.value}`
})

// Keep `draftId` in sync with the draft's id for the scope getter above.
watch(
  () => form.draft.value?.id,
  (value) => {
    draftId.value = value ?? ''
  }
)

// If-Match seeding: createBot is a POST, but the backend still expects the
// current reload generation as the optimistic-concurrency token. Mirror
// useConfigEditor minimally — poll reload-status, load the template, then stamp
// the observed generation so the POST carries a fresh If-Match.
onMounted(async () => {
  await status.pollStatus()
  await form.load()
  if (status.generation.value != null) form.setGeneration(status.generation.value)
})

onBeforeRouteLeave(() => {
  if (form.dirty.value && !window.confirm('Discard unsaved changes?')) return false
  return true
})

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

const selectedAccount = computed<EffectiveAccountConfig | null>(
  () => accounts.value.find((a) => a.id === form.draft.value?.account) ?? null
)
const inheritedRisk = computed<Partial<RiskProfileConfig>>(
  () => riskProfiles.value.find((p) => p.id === form.draft.value?.risk.profile) ?? {}
)
const budgetUsdHint = computed<string | null>(() => {
  const acct = selectedAccount.value
  const pct = form.draft.value?.budget.pct
  if (!acct || typeof pct !== 'number' || !Number.isFinite(pct)) return null
  return `≈ ${fmtNumber((pct / 100) * acct.total_budget_usd, 2)} USD of ${acct.id}`
})

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
            <span class="font-data text-[13px] text-ink">new</span>
          </div>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1100px] mx-auto w-full">
        <PageHeader
          eyebrow="New bot"
          title="Create bot"
          description="Define a new trading instance. It is created disabled — start it from the runtime page once configured."
        />

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
          @reload="form.load"
        >
          <div
            v-if="form.draft.value"
            class="space-y-8"
          >
            <!-- Identity & structure -->
            <section class="surface-card p-5 space-y-4">
              <SectionHeader
                label="Identity & structure"
                hint="Defines what the bot trades and how. The id becomes the on-disk filename."
              />
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                <ConfigField
                  label="Bot id"
                  hint="letters, numbers, - and _"
                  :error="form.errorFor('id')"
                >
                  <UInput
                    v-model="form.draft.value.id"
                    placeholder="e.g. btc-trend"
                    class="font-data w-full"
                  />
                </ConfigField>
                <ConfigField
                  label="Market"
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

            <!-- Indicators -->
            <section class="surface-card p-5 space-y-4">
              <SectionHeader
                label="Indicators"
                hint="The indicator stack feeding the strategy. Optional — add them now or later."
              />
              <ConfigIndicatorList
                v-model="form.draft.value.indicators"
                :errors="form.errorFor"
              />
            </section>

            <!-- Runtime toggles -->
            <section class="surface-card p-5 space-y-4">
              <SectionHeader
                label="Runtime"
                hint="The bot is created disabled; enable + start it from the runtime page."
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

            <!-- Execution constraints -->
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

            <!-- Risk -->
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
      </div>
    </template>
  </UDashboardPanel>
</template>
