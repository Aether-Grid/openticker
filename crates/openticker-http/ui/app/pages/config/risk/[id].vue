<script setup lang="ts">
import type { BotConfig, RiskProfileConfig } from '~/types/api'
import { validateRiskProfile } from '~/utils/configValidation'

definePageMeta({ layout: 'default' })

const route = useRoute()
const id = computed(() => route.params.id as string)

const actions = useConfigActions()

// Stash the effective bots list at load time so the header can show which bots
// reference this profile, without a second fetch on every poll.
const bots = ref<BotConfig[]>([])

const { form, notFound, reload } = useConfigEditor<RiskProfileConfig>({
  load: async () => {
    const eff = await actions.fetchEffective()
    bots.value = eff.bots ?? []
    return eff.risk_profiles.find((p) => p.id === id.value) ?? null
  },
  // useConfigForm.submit passes the generation observed at the last load.
  save: (draft, generation) => actions.saveRisk(id.value, draft, generation),
  validate: validateRiskProfile
  // No scope: risk profiles have no change-set scope on the backend, so any 409
  // falls through to the conflict banner rather than mapping to a field.
})

// Bots that reference this risk profile. BotConfig.risk is non-optional.
const usedBy = computed<BotConfig[]>(() => bots.value.filter((b) => b.risk.profile === id.value))
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
            <span class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data">risk</span>
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
          title="Risk profile not found in config"
          :description="`No risk profile with id '${id}' is present in the effective configuration.`"
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
            eyebrow="Risk profile"
            :title="id"
            description="A reusable risk envelope bots can reference. Edits apply to every bot using this profile."
          >
            <template #actions>
              <div class="text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data text-right">
                Used by {{ usedBy.length }} {{ usedBy.length === 1 ? 'bot' : 'bots' }}
                <div
                  v-if="usedBy.length"
                  class="mt-1 flex flex-wrap justify-end gap-1.5 normal-case tracking-normal"
                >
                  <NuxtLink
                    v-for="bot in usedBy"
                    :key="bot.id"
                    :to="`/config/bots/${bot.id}`"
                    class="text-ink hover:underline"
                  >
                    {{ bot.id }}
                  </NuxtLink>
                </div>
              </div>
            </template>
          </PageHeader>

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
              <div class="surface-card p-5 space-y-5">
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                  <ConfigField
                    label="Profile id"
                    :locked="true"
                    lock-reason="Risk profile id is immutable"
                  >
                    <UInput
                      :model-value="form.draft.value.id"
                      class="font-data w-full"
                    />
                  </ConfigField>
                </div>

                <ConfigRiskFields
                  v-model="form.draft.value"
                  :nullable="false"
                  :errors="form.errorFor"
                />
              </div>
            </div>
          </ConfigFormShell>
        </template>
      </div>
    </template>
  </UDashboardPanel>
</template>
