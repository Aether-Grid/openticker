<script setup lang="ts">
import type { AccountConfigUpdate, EffectiveAccountConfig, ExecutionMode } from '~/types/api'
import { validateAccount } from '~/utils/configValidation'

definePageMeta({ layout: 'default' })

const route = useRoute()
const id = computed(() => route.params.id as string)

const actions = useConfigActions()
const status = useConfigStatus()

const form = useConfigForm<EffectiveAccountConfig>({
  load: async () => {
    const eff = await actions.fetchEffective()
    return eff.accounts.find((a) => a.id === id.value) ?? null
  },
  // Map the read shape (EffectiveAccountConfig) down to the write shape
  // (AccountConfigUpdate): drop `id` and `secret_status`, never send any *_env
  // field. An empty reconciliation_base_url becomes null.
  save: (draft, generation) => {
    const body: AccountConfigUpdate = {
      kind: draft.kind,
      mode: draft.mode,
      use_demo_mode: draft.use_demo_mode,
      reconciliation_remote_snapshot: draft.reconciliation_remote_snapshot,
      execution_remote_submission: draft.execution_remote_submission,
      reconciliation_base_url:
        draft.reconciliation_base_url && draft.reconciliation_base_url.trim().length > 0
          ? draft.reconciliation_base_url
          : null,
      cash_balance_assets: draft.cash_balance_assets,
      total_budget_usd: draft.total_budget_usd
    }
    return actions.saveAccount(id.value, body, generation)
  },
  // validateAccount expects the write shape; the editable/restart-scoped fields
  // it checks (kind, mode, total_budget_usd) are present on the draft too.
  validate: (draft) => validateAccount(draft as unknown as AccountConfigUpdate),
  // Account scope lets account-scoped 409s be attributed to this page. Codes
  // like account_settings_changed have no field mapping, so they still render in
  // the conflict banner — which is the intended contract for restart-scoped edits.
  scope: () => `account:${id.value}`
})

// Seed the If-Match token from the shared reload-status generation and keep it
// fresh: a clean form silently reloads on an out-of-band change; a dirty form
// raises the stale banner instead.
watch(
  () => status.generation.value,
  (g) => {
    if (g != null) form.onGenerationChange(g)
  },
  { immediate: true }
)
useAutoRefresh(status.poll)

onMounted(() => {
  void status.pollStatus()
  void form.load()
})

const notFound = computed(() => !form.pending.value && form.original.value === null)

// UInput v-model is string-typed; the draft field is `string | null`. Proxy the
// null<->'' mapping here so the input never receives null. The save callback
// re-collapses an empty string back to null.
const reconciliationBaseUrl = computed<string>({
  get: () => form.draft.value?.reconciliation_base_url ?? '',
  set: (value) => {
    if (form.draft.value) form.draft.value.reconciliation_base_url = value.length > 0 ? value : null
  }
})

const modeItems = [
  { label: 'paper', value: 'paper' as ExecutionMode },
  { label: 'live', value: 'live' as ExecutionMode }
]

const RESTART_HINT = 'Changing this requires a service restart (returns a conflict if changed while running).'

onBeforeRouteLeave(() => {
  if (form.dirty.value && !window.confirm('Discard unsaved changes?')) return false
  return true
})
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
            <span class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data">accounts</span>
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
          title="Account not found in config"
          :description="`No account with id '${id}' is present in the effective configuration.`"
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
            eyebrow="Account"
            :title="id"
            description="Budgets and balances apply live; mode and connectivity settings take effect on the next service restart."
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
              <!-- Identity (locked) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Identity"
                  hint="Immutable account identifiers."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                  <ConfigField
                    label="Account id"
                    :locked="true"
                    lock-reason="immutable"
                  >
                    <UInput
                      :model-value="form.draft.value.id"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Kind"
                    :locked="true"
                    lock-reason="immutable; requires restart"
                  >
                    <UInput
                      :model-value="form.draft.value.kind"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Budget & balances (hot-writable) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Budget & balances"
                  hint="Applied immediately on save — no restart required."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <ConfigField
                    label="Total budget USD"
                    :error="form.errorFor('total_budget_usd')"
                  >
                    <UInputNumber
                      v-model="form.draft.value.total_budget_usd"
                      :min="0"
                      :step="1"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Cash balance assets"
                    hint="Press Enter to add"
                  >
                    <ConfigStringList
                      v-model="form.draft.value.cash_balance_assets"
                      :uppercase="true"
                      placeholder="USDT, USD…"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Execution & connectivity (restart-scoped) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Execution & connectivity"
                  hint="Restart-scoped — saving while the service is running returns a conflict."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <ConfigField
                    label="Mode"
                    :hint="RESTART_HINT"
                  >
                    <USelect
                      v-model="form.draft.value.mode"
                      :items="modeItems"
                      class="font-data w-full"
                    />
                    <p
                      v-if="form.draft.value.mode === 'live'"
                      class="mt-2 text-[11.5px] text-[color:var(--color-pastel-red-ink)]"
                    >
                      Live mode places real orders. The runtime also requires
                      <span class="font-data">safety.require_explicit_live_enable</span> before it will trade live.
                    </p>
                  </ConfigField>
                  <ConfigField
                    label="Reconciliation base URL"
                    :hint="RESTART_HINT"
                  >
                    <UInput
                      v-model="reconciliationBaseUrl"
                      placeholder="(none)"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                  <ConfigField
                    label="Use demo mode"
                    :hint="RESTART_HINT"
                  >
                    <USwitch v-model="form.draft.value.use_demo_mode" />
                  </ConfigField>
                  <ConfigField
                    label="Remote reconciliation snapshot"
                    :hint="RESTART_HINT"
                  >
                    <USwitch v-model="form.draft.value.reconciliation_remote_snapshot" />
                  </ConfigField>
                  <ConfigField
                    label="Remote execution submission"
                    :hint="RESTART_HINT"
                  >
                    <USwitch v-model="form.draft.value.execution_remote_submission" />
                  </ConfigField>
                </div>
              </section>

              <!-- Credentials (read-only presence) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Credentials"
                  hint="Presence only — never editable here."
                />
                <div class="flex flex-wrap items-center gap-3">
                  <div class="flex items-center gap-2">
                    <span class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data">API key</span>
                    <StatusPill
                      :label="form.draft.value.secret_status.api_key_present ? 'set' : 'missing'"
                      :tone="form.draft.value.secret_status.api_key_present ? 'green' : 'red'"
                    />
                  </div>
                  <div class="flex items-center gap-2">
                    <span class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data">API secret</span>
                    <StatusPill
                      :label="form.draft.value.secret_status.api_secret_present ? 'set' : 'missing'"
                      :tone="form.draft.value.secret_status.api_secret_present ? 'green' : 'red'"
                    />
                  </div>
                  <div class="flex items-center gap-2">
                    <span class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data">Passphrase</span>
                    <StatusPill
                      :label="form.draft.value.secret_status.passphrase_present ? 'set' : 'missing'"
                      :tone="form.draft.value.secret_status.passphrase_present ? 'green' : 'red'"
                    />
                  </div>
                </div>
                <p class="text-[12px] text-ink-soft">
                  Credentials are environment-variable references managed on disk; they are never editable here.
                </p>
              </section>
            </div>
          </ConfigFormShell>
        </template>
      </div>
    </template>
  </UDashboardPanel>
</template>
