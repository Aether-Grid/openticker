<script setup lang="ts">
import type { GlobalConfig } from '~/types/api'
import { validateGlobal } from '~/utils/configValidation'

definePageMeta({ layout: 'default' })

const actions = useConfigActions()

// Stash the effective account-id list at load time so the watchlist editor can
// offer account options without a second fetch on every poll.
const accountIds = ref<string[]>([])

const { form, notFound, reload } = useConfigEditor<GlobalConfig>({
  load: async () => {
    const eff = await actions.fetchEffective()
    accountIds.value = (eff.accounts ?? []).map((a) => a.id)
    return eff.global ?? null
  },
  // useConfigForm.submit passes the generation observed at the last load. The
  // PUT sends the whole GlobalConfig; locked fields pass through unchanged.
  save: (draft, generation) => actions.saveGlobal(draft, generation),
  validate: validateGlobal,
  // Global scope lets global-scoped 409s (e.g. storage_changed, bot_dir_changed)
  // be attributed to this page; they have no field mapping so they render in the
  // conflict banner.
  scope: 'global'
})

// log_level is a free-form String on the backend; offer the five standard
// tracing levels as a convenience while keeping the select value a plain string.
const logLevelItems = [
  { label: 'trace', value: 'trace' },
  { label: 'debug', value: 'debug' },
  { label: 'info', value: 'info' },
  { label: 'warn', value: 'warn' },
  { label: 'error', value: 'error' }
]

const SERVICE_LOCK = 'Requires service restart'
const HTTP_LOCK = 'Requires service restart; changing the bind would sever the dashboard'
const STORAGE_LOCK = 'Requires service restart'
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
            <span class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data">global</span>
          </div>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="px-8 py-10 max-w-[1100px] mx-auto w-full">
        <EmptyState
          v-if="notFound"
          icon="i-lucide-search-x"
          title="Global config unavailable"
          description="The effective configuration did not include a global section."
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
            eyebrow="Global config"
            title="Global"
            description="Service-wide settings. Most fields are restart-scoped and locked; observability, safety, and data-plane defaults apply on save."
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
            @reload="reload"
          >
            <div
              v-if="form.draft.value"
              class="space-y-8"
            >
              <!-- Service (locked) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Service"
                  hint="Immutable at runtime — changes require a service restart."
                />
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                  <ConfigField
                    label="Environment"
                    :locked="true"
                    :lock-reason="SERVICE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.service.environment"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Data directory"
                    :locked="true"
                    :lock-reason="SERVICE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.service.data_dir"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Bot directory"
                    :locked="true"
                    :lock-reason="SERVICE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.service.bot_dir"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- HTTP (locked) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="HTTP"
                  hint="Dashboard transport — restart-scoped and locked."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                  <ConfigField
                    label="Enabled"
                    :locked="true"
                    :lock-reason="HTTP_LOCK"
                  >
                    <USwitch :model-value="form.draft.value.http.enabled" />
                  </ConfigField>
                  <ConfigField
                    label="Bind"
                    :locked="true"
                    :lock-reason="HTTP_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.http.bind"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Request log"
                    :locked="true"
                    :lock-reason="HTTP_LOCK"
                  >
                    <USwitch :model-value="form.draft.value.http.request_log" />
                  </ConfigField>
                  <ConfigField
                    label="OpenAPI enabled"
                    :locked="true"
                    :lock-reason="HTTP_LOCK"
                  >
                    <USwitch :model-value="form.draft.value.http.openapi_enabled" />
                  </ConfigField>
                  <ConfigField
                    label="OpenAPI path"
                    :locked="true"
                    :lock-reason="HTTP_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.http.openapi_path"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Storage (locked) -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Storage"
                  hint="Persistence backend — restart-scoped and locked."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                  <ConfigField
                    label="Kind"
                    :locked="true"
                    :lock-reason="STORAGE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.storage.kind"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Path"
                    :locked="true"
                    :lock-reason="STORAGE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.storage.path"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Busy timeout ms"
                    :locked="true"
                    :lock-reason="STORAGE_LOCK"
                  >
                    <UInputNumber
                      :model-value="form.draft.value.storage.busy_timeout_ms"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Prune removed bots on startup"
                    :locked="true"
                    :lock-reason="STORAGE_LOCK"
                  >
                    <USwitch :model-value="form.draft.value.storage.prune_removed_bots_on_startup" />
                  </ConfigField>
                </div>
              </section>

              <!-- Observability -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Observability"
                  hint="Log level applies on save; metrics transport is restart-scoped."
                />
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                  <ConfigField
                    label="Log level"
                    :error="form.errorFor('observability.log_level')"
                  >
                    <USelect
                      v-model="form.draft.value.observability.log_level"
                      :items="logLevelItems"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Metrics enabled"
                    :locked="true"
                    :lock-reason="SERVICE_LOCK"
                  >
                    <USwitch :model-value="form.draft.value.observability.metrics_enabled" />
                  </ConfigField>
                  <ConfigField
                    label="Metrics path"
                    :locked="true"
                    :lock-reason="SERVICE_LOCK"
                  >
                    <UInput
                      :model-value="form.draft.value.observability.metrics_path"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>
              </section>

              <!-- Safety -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Safety"
                  hint="Live-trading guardrails — applied on save."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <ConfigField label="Require explicit live enable">
                    <USwitch v-model="form.draft.value.safety.require_explicit_live_enable" />
                    <p
                      v-if="!form.draft.value.safety.require_explicit_live_enable"
                      class="mt-2 text-[11.5px] text-[color:var(--color-pastel-red-ink)]"
                    >
                      Disabling this allows bots to trade live without explicit per-bot opt-in.
                    </p>
                  </ConfigField>
                  <ConfigField label="Start paused if recovery uncertain">
                    <USwitch v-model="form.draft.value.safety.default_start_paused_if_recovery_uncertain" />
                  </ConfigField>
                </div>
              </section>

              <!-- Data plane -->
              <section class="surface-card p-5 space-y-4">
                <SectionHeader
                  label="Data plane"
                  hint="Polling and retention defaults plus the per-symbol watchlist."
                />
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <ConfigField
                    label="Default polling interval ms"
                    :error="form.errorFor('data_plane.default_polling_interval_ms')"
                  >
                    <UInputNumber
                      v-model="form.draft.value.data_plane.default_polling_interval_ms"
                      :min="250"
                      :step="250"
                      class="font-data w-full"
                    />
                  </ConfigField>
                  <ConfigField
                    label="Default retention"
                    :error="form.errorFor('data_plane.default_retention')"
                  >
                    <UInputNumber
                      v-model="form.draft.value.data_plane.default_retention"
                      :min="1"
                      :step="1"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>

                <div class="space-y-3 pt-1">
                  <SectionHeader
                    label="Watchlist"
                    hint="Per-symbol data subscriptions. Polling and retention overrides inherit the defaults above when left empty."
                  />
                  <ConfigWatchlistEditor
                    v-model="form.draft.value.data_plane.watchlist"
                    :accounts="accountIds"
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
