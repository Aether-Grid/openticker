<script setup lang="ts">
import type { BotConfig, EffectiveAccountConfig, EffectiveConfig, OpenApiSpec, RiskProfileConfig } from '~/types/api'
import { fmtNumber, fmtRelativeMs, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const actions = useServiceActions()
const configActions = useConfigActions()
const status = useConfigStatus()
const router = useRouter()

const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)
const effective = shallowRef<EffectiveConfig | null>(null)
const matrix = shallowRef<unknown>(null)
const openapi = shallowRef<OpenApiSpec | null>(null)
const metrics = ref<string>('')

const search = ref('')
const methodFilter = ref('all')
const selectedPath = ref<string | null>(null)

const load = async () => {
  pending.value = true
  try {
    const [cfg, m, spec, met] = await Promise.all([
      api<EffectiveConfig>('/v1/config/effective').catch(() => null),
      api<unknown>('/v1/connectors/matrix').catch(() => null),
      api<OpenApiSpec>('/openapi.json').catch(() => null),
      api<string>('/metrics', { responseType: 'text' }).catch(() => '')
    ])
    effective.value = cfg
    matrix.value = m
    openapi.value = spec
    metrics.value = typeof met === 'string' ? met : ''
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
useAutoRefresh(status.poll)
onMounted(() => {
  void load()
  void status.poll()
})

type RouteRow = { method: string; path: string; operation: string; summary: string }
const routes = computed<RouteRow[]>(() => {
  const rows: RouteRow[] = []
  const paths = openapi.value?.paths ?? {}
  for (const [p, methods] of Object.entries(paths)) {
    for (const [method, op] of Object.entries(methods)) {
      rows.push({
        method: method.toUpperCase(),
        path: p,
        operation: op.operationId ?? '—',
        summary: op.summary ?? ''
      })
    }
  }
  return rows.sort((a, b) => a.path.localeCompare(b.path))
})

const methodItems = computed(() => [
  { label: 'All methods', value: 'all' },
  ...Array.from(new Set(routes.value.map((r) => r.method))).map((m) => ({ label: m, value: m }))
])

const filteredRoutes = computed(() => {
  const term = search.value.trim().toLowerCase()
  return routes.value.filter((r) => {
    if (methodFilter.value !== 'all' && r.method !== methodFilter.value) return false
    if (term) {
      const hay = [r.path, r.operation, r.summary].join(' ').toLowerCase()
      if (!hay.includes(term)) return false
    }
    return true
  })
})

const routeCols = [
  { key: 'method', label: 'Method', width: '90px' },
  { key: 'path', label: 'Path' },
  { key: 'operation', label: 'Operation' },
  { key: 'summary', label: 'Summary' }
]

const selectedRoute = computed(
  () => filteredRoutes.value.find((r) => r.path === selectedPath.value) ?? filteredRoutes.value[0] ?? null
)

function methodTone(method: string): 'green' | 'blue' | 'yellow' | 'red' | 'neutral' {
  switch (method) {
    case 'GET':
      return 'blue'
    case 'POST':
      return 'green'
    case 'PUT':
      return 'yellow'
    case 'DELETE':
      return 'red'
    default:
      return 'neutral'
  }
}

const cfg = computed(() => effective.value ?? null)
const service = computed(() => cfg.value?.global?.service ?? null)
// NOTE: the API exposes accounts under `accounts`, risk profiles under
// `risk_profiles` (NOT `risk`), and bots under `bots`. A prior version of this
// page read `cfg.risk`, which silently rendered an empty risk list.
const accounts = computed<EffectiveAccountConfig[]>(() => cfg.value?.accounts ?? [])
const bots = computed<BotConfig[]>(() => cfg.value?.bots ?? [])
const riskProfiles = computed<RiskProfileConfig[]>(() => cfg.value?.risk_profiles ?? [])

const reloadLast = computed(() => status.last.value)
const reloadOutcomeTone = computed<'green' | 'red' | 'neutral'>(() => {
  const outcome = reloadLast.value?.outcome
  if (outcome === 'reloaded' || outcome === 'no_change') return 'green'
  if (outcome === 'rejected' || outcome === 'failed') return 'red'
  return 'neutral'
})

const botCols = [
  { key: 'id', label: 'Bot' },
  { key: 'enabled', label: 'Enabled' },
  { key: 'account', label: 'Account' },
  { key: 'timeframe', label: 'Timeframe' },
  { key: 'strategy', label: 'Strategy' },
  { key: 'actions', label: '', align: 'right' as const, width: '64px' }
]
const accountCols = [
  { key: 'id', label: 'Account' },
  { key: 'kind', label: 'Kind' },
  { key: 'mode', label: 'Mode' },
  { key: 'total_budget_usd', label: 'Budget USD', align: 'right' as const }
]
const riskCols = [
  { key: 'id', label: 'Profile' },
  { key: 'max_daily_loss_pct', label: 'Max daily loss %', align: 'right' as const },
  { key: 'max_order_notional_usd', label: 'Max order notional USD', align: 'right' as const }
]

const configPanel = ref<'config' | 'metrics' | 'openapi' | 'route'>('config')

function selectRoute(path: string) {
  selectedPath.value = path
  configPanel.value = 'route'
}

// --- Bot deletion ----------------------------------------------------------
const deleteOpen = ref(false)
const deleteTarget = shallowRef<BotConfig | null>(null)
const deleting = ref(false)
const deleteError = ref<string | null>(null)

function askDelete(bot: BotConfig) {
  deleteTarget.value = bot
  deleteError.value = null
  deleteOpen.value = true
}

async function confirmDelete() {
  const bot = deleteTarget.value
  if (!bot) return
  deleting.value = true
  deleteError.value = null
  try {
    const result = await configActions.deleteBot(bot.id, status.generation.value)
    if (result.ok) {
      deleteOpen.value = false
      deleteTarget.value = null
      await Promise.all([load(), status.poll()])
    } else {
      // 409 (bot running / duplicate) carries violations; 422 carries `error`.
      deleteError.value =
        result.violations?.map((v) => v.message).join('; ') || result.error || 'Could not delete this bot.'
    }
  } finally {
    deleting.value = false
  }
}

async function reloadFromDisk() {
  await actions.reloadConfig()
  await Promise.all([load(), status.poll()])
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
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Config</span>
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
          eyebrow="Configuration"
          title="What the runtime is operating under."
          description="Edit accounts, bots, risk profiles and global settings — plus the live route and metrics explorer."
        >
          <template #actions>
            <UButton
              color="neutral"
              variant="solid"
              size="sm"
              icon="i-lucide-plus"
              label="New bot"
              @click="router.push('/config/bots/new')"
            />
            <UButton
              color="neutral"
              variant="outline"
              size="sm"
              icon="i-lucide-rotate-ccw"
              label="Reload from disk"
              @click="reloadFromDisk"
            />
          </template>
        </PageHeader>

        <!-- Reload-status strip -->
        <section class="surface-card px-5 py-3 flex flex-wrap items-center gap-x-6 gap-y-2">
          <div class="flex items-center gap-2">
            <span class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Last reload</span>
            <StatusPill
              v-if="reloadLast"
              :label="reloadLast.outcome"
              :tone="reloadOutcomeTone"
            />
            <span
              v-else
              class="text-[12.5px] text-ink-soft font-data"
              >no reloads yet</span
            >
          </div>
          <div
            v-if="reloadLast"
            class="flex items-center gap-2 text-[12.5px] text-ink-soft"
          >
            <span class="font-data">{{ fmtRelativeMs(reloadLast.at_ms) }}</span>
            <span class="text-ink-soft/60">·</span>
            <span class="font-data uppercase tracking-[0.08em] text-[11px]">{{ reloadLast.trigger }}</span>
          </div>
          <div
            v-if="reloadLast?.error"
            class="text-[12px] text-[color:var(--color-pastel-red-ink)] truncate max-w-md"
            :title="reloadLast.error"
          >
            {{ reloadLast.error }}
          </div>
          <div class="ml-auto flex items-center gap-2">
            <span class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Generation</span>
            <span class="font-data tabular-nums text-[13px]">{{ status.generation.value }}</span>
          </div>
        </section>

        <section class="grid grid-cols-12 gap-3">
          <NuxtLink
            to="/config/global"
            class="col-span-12 md:col-span-4 surface-card p-5 hover:bg-white transition-colors group"
          >
            <div class="flex items-center justify-between">
              <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Global config</div>
              <UIcon
                name="i-lucide-arrow-up-right"
                class="size-4 text-ink-soft opacity-0 group-hover:opacity-100 transition-opacity"
              />
            </div>
            <div class="mt-2 font-editorial text-[22px]">
              {{ service?.environment ?? '—' }}
            </div>
            <div class="text-[11.5px] text-ink-soft mt-1 font-data truncate">
              Data dir {{ service?.data_dir ?? '—' }}
            </div>
            <div class="text-[11.5px] text-ink-soft mt-0.5 font-data truncate">
              Bot dir {{ service?.bot_dir ?? '—' }}
            </div>
          </NuxtLink>
          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Accounts"
              :value="accounts.length"
              accent="blue"
            />
          </div>
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Bots"
              :value="bots.length"
              accent="green"
            />
          </div>
          <div class="col-span-6 md:col-span-3">
            <MetricCard
              label="Risk profiles"
              :value="riskProfiles.length"
              accent="violet"
            />
          </div>
        </section>

        <!-- Bots -->
        <section class="space-y-4">
          <SectionHeader
            label="Bots"
            hint="Click to edit an instance. Each row links to its full editor."
          />
          <DataTable
            :columns="botCols"
            :rows="bots"
            :row-key="(r: BotConfig) => r.id"
            @row-click="(row: BotConfig) => router.push(`/config/bots/${row.id}`)"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'id'">
                <div class="font-editorial text-[16px] leading-tight">
                  {{ (row as BotConfig).id }}
                </div>
                <div class="text-[11px] text-ink-soft font-data truncate">
                  {{ ((row as BotConfig).symbols ?? []).join(' · ') || '—' }}
                </div>
              </template>
              <template v-else-if="column.key === 'enabled'">
                <StatusPill
                  :label="(row as BotConfig).enabled ? 'enabled' : 'disabled'"
                  :tone="(row as BotConfig).enabled ? 'green' : 'neutral'"
                />
              </template>
              <template v-else-if="column.key === 'account'">
                <span class="font-data text-[12.5px]">{{ (row as BotConfig).account ?? '—' }}</span>
              </template>
              <template v-else-if="column.key === 'timeframe'">
                <span class="font-data text-[12.5px]">{{ (row as BotConfig).timeframe ?? '—' }}</span>
              </template>
              <template v-else-if="column.key === 'strategy'">
                <span class="font-data text-[12.5px]">{{ short((row as BotConfig).strategy, 28) }}</span>
              </template>
              <template v-else-if="column.key === 'actions'">
                <UTooltip text="Delete bot">
                  <UButton
                    color="error"
                    variant="ghost"
                    size="xs"
                    icon="i-lucide-trash-2"
                    aria-label="Delete bot"
                    @click.stop="askDelete(row as BotConfig)"
                  />
                </UTooltip>
              </template>
            </template>
            <template #empty> No bots configured. Create one with the New bot button. </template>
          </DataTable>
        </section>

        <!-- Accounts -->
        <section class="space-y-4">
          <SectionHeader
            label="Accounts"
            hint="Trading accounts and their budgets."
          />
          <DataTable
            :columns="accountCols"
            :rows="accounts"
            :row-key="(r: EffectiveAccountConfig) => r.id"
            @row-click="(row: EffectiveAccountConfig) => router.push(`/config/accounts/${row.id}`)"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'id'">
                <span class="font-editorial text-[16px]">{{ (row as EffectiveAccountConfig).id }}</span>
              </template>
              <template v-else-if="column.key === 'kind'">
                <span class="font-data text-[12.5px]">{{ (row as EffectiveAccountConfig).kind ?? '—' }}</span>
              </template>
              <template v-else-if="column.key === 'mode'">
                <StatusPill
                  :label="(row as EffectiveAccountConfig).mode"
                  :tone="(row as EffectiveAccountConfig).mode === 'live' ? 'red' : 'neutral'"
                />
              </template>
              <template v-else-if="column.key === 'total_budget_usd'">
                <span class="font-data tabular-nums text-[13px]">
                  {{ fmtNumber((row as EffectiveAccountConfig).total_budget_usd ?? 0, 2) }}
                </span>
              </template>
            </template>
            <template #empty> No accounts configured. </template>
          </DataTable>
        </section>

        <!-- Risk profiles -->
        <section class="space-y-4">
          <SectionHeader
            label="Risk profiles"
            hint="Reusable risk envelopes bots can reference."
          />
          <DataTable
            :columns="riskCols"
            :rows="riskProfiles"
            :row-key="(r: RiskProfileConfig) => r.id"
            @row-click="(row: RiskProfileConfig) => router.push(`/config/risk/${row.id}`)"
          >
            <template #cell="{ row, column }">
              <template v-if="column.key === 'id'">
                <span class="font-editorial text-[16px]">{{ (row as RiskProfileConfig).id }}</span>
              </template>
              <template v-else-if="column.key === 'max_daily_loss_pct'">
                <span class="font-data tabular-nums text-[13px]">
                  {{ fmtNumber((row as RiskProfileConfig).max_daily_loss_pct ?? 0, 2) }}
                </span>
              </template>
              <template v-else-if="column.key === 'max_order_notional_usd'">
                <span class="font-data tabular-nums text-[13px]">
                  {{ fmtNumber((row as RiskProfileConfig).max_order_notional_usd ?? 0, 2) }}
                </span>
              </template>
            </template>
            <template #empty> No risk profiles configured. </template>
          </DataTable>
        </section>

        <section class="space-y-4">
          <div class="flex flex-wrap items-center gap-3">
            <SectionHeader
              label="Route explorer"
              hint="Every HTTP endpoint in the runtime."
            />
            <div class="flex flex-wrap items-center gap-2 ml-auto">
              <UInput
                v-model="search"
                icon="i-lucide-search"
                placeholder="Filter routes..."
                size="sm"
                class="w-64"
              />
              <USelect
                v-model="methodFilter"
                :items="methodItems"
                size="sm"
                class="w-36"
              />
            </div>
          </div>

          <div class="surface-card overflow-hidden">
            <div class="overflow-x-auto max-h-[420px] overflow-y-auto">
              <table class="w-full text-[13px] border-collapse">
                <thead class="sticky top-0 bg-white">
                  <tr class="text-left">
                    <th
                      v-for="col in routeCols"
                      :key="col.key"
                      class="px-4 py-2.5 text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data font-medium hairline-b"
                    >
                      {{ col.label }}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="r in filteredRoutes"
                    :key="`${r.method} ${r.path}`"
                    class="hairline-b last:border-b-0 hover:bg-[color:var(--color-canvas)] cursor-pointer transition-colors"
                    :class="[selectedPath === r.path ? 'bg-[color:var(--color-canvas)]' : '']"
                    @click="selectRoute(r.path)"
                  >
                    <td class="px-4 py-2.5">
                      <StatusPill
                        :label="r.method"
                        :tone="methodTone(r.method)"
                      />
                    </td>
                    <td class="px-4 py-2.5 font-data text-[12.5px]">
                      {{ r.path }}
                    </td>
                    <td class="px-4 py-2.5 font-data text-[12px] text-ink-soft">
                      {{ short(r.operation, 36) }}
                    </td>
                    <td class="px-4 py-2.5 text-[12.5px] text-ink-soft truncate">
                      {{ r.summary }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </section>

        <section class="space-y-4">
          <div class="flex flex-wrap gap-1.5">
            <button
              v-for="p in [
                { label: 'Config JSON', value: 'config' as const },
                { label: 'Metrics', value: 'metrics' as const },
                { label: 'OpenAPI', value: 'openapi' as const },
                { label: 'Selected route', value: 'route' as const }
              ]"
              :key="p.value"
              type="button"
              class="inline-flex items-center gap-2 px-3 py-1.5 rounded-md text-[12px] tracking-tight transition-colors"
              :class="configPanel === p.value ? 'bg-ink text-white' : 'surface-card hover:bg-white'"
              @click="configPanel = p.value"
            >
              {{ p.label }}
            </button>
          </div>

          <JsonBlock
            v-if="configPanel === 'config'"
            :value="cfg ?? {}"
            max-height="560px"
          />
          <JsonBlock
            v-else-if="configPanel === 'openapi'"
            :value="openapi ?? {}"
            max-height="560px"
          />
          <JsonBlock
            v-else-if="configPanel === 'route'"
            :value="selectedRoute ?? { hint: 'Select a route above.' }"
            max-height="360px"
          />
          <pre
            v-else
            class="surface-card font-data text-[11.5px] leading-relaxed text-ink overflow-auto p-4 tabular-nums whitespace-pre"
            style="max-height: 560px"
            >{{ metrics || 'Metrics not available.' }}</pre
          >
        </section>

        <UModal
          v-model:open="deleteOpen"
          :ui="{
            content: 'rounded-xl max-w-[480px]',
            header: 'border-b border-[color:var(--color-hairline)] px-6 py-4',
            body: 'px-6 py-5',
            footer: 'border-t border-[color:var(--color-hairline)] px-6 py-3'
          }"
        >
          <template #header>
            <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Delete bot</div>
            <div class="font-editorial text-[22px] leading-tight mt-0.5">Remove {{ deleteTarget?.id }}?</div>
          </template>
          <template #body>
            <p class="text-[13px] text-ink-soft">
              This deletes the bot's configuration file. The runtime will reload and stop tracking this instance. A
              running bot must be stopped first.
            </p>
            <UAlert
              v-if="deleteError"
              class="mt-4"
              color="error"
              variant="subtle"
              icon="i-lucide-octagon-alert"
              :title="deleteError"
              :ui="{ root: 'rounded-lg' }"
            />
          </template>
          <template #footer>
            <div class="flex items-center justify-end w-full gap-2">
              <UButton
                color="neutral"
                variant="ghost"
                size="sm"
                label="Cancel"
                @click="deleteOpen = false"
              />
              <UButton
                color="error"
                variant="solid"
                size="sm"
                icon="i-lucide-trash-2"
                label="Delete bot"
                :loading="deleting"
                @click="confirmDelete"
              />
            </div>
          </template>
        </UModal>
      </div>
    </template>
  </UDashboardPanel>
</template>
