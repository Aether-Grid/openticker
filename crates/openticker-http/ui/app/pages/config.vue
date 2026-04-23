<script setup lang="ts">
import type { OpenApiSpec } from '~/types/api'
import { short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const actions = useServiceActions()

const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)
const effective = shallowRef<Record<string, unknown> | null>(null)
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
      api<Record<string, unknown>>('/v1/config/effective').catch(() => null),
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
onMounted(() => {
  void load()
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

const cfg = computed(() => effective.value ?? {})
const service = computed(
  () => ((cfg.value as Record<string, unknown>).service as Record<string, unknown> | undefined) ?? {}
)
const accounts = computed(
  () => ((cfg.value as Record<string, unknown>).accounts as Record<string, unknown>[] | undefined) ?? []
)
const bots = computed(
  () => ((cfg.value as Record<string, unknown>).bots as Record<string, unknown>[] | undefined) ?? []
)
const risk = computed(
  () => ((cfg.value as Record<string, unknown>).risk as Record<string, unknown>[] | undefined) ?? []
)

const configPanel = ref<'config' | 'metrics' | 'openapi' | 'route'>('config')

function selectRoute(path: string) {
  selectedPath.value = path
  configPanel.value = 'route'
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
          description="Live config snapshot, accounts, bots, risk profiles, routes, and metrics."
        >
          <template #actions>
            <UButton
              color="neutral"
              variant="solid"
              size="sm"
              icon="i-lucide-rotate-ccw"
              label="Reload from disk"
              @click="actions.reloadConfig().then(load)"
            />
          </template>
        </PageHeader>

        <section class="grid grid-cols-12 gap-3">
          <div class="col-span-12 md:col-span-4 surface-card p-5">
            <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Service</div>
            <div class="mt-2 font-editorial text-[22px]">
              {{ service.environment ?? '—' }}
            </div>
            <div class="text-[11.5px] text-ink-soft mt-1 font-data truncate">
              Data dir {{ service.data_dir ?? '—' }}
            </div>
            <div class="text-[11.5px] text-ink-soft mt-0.5 font-data truncate">
              Bot dir {{ service.bot_dir ?? '—' }}
            </div>
          </div>
          <div class="col-span-6 md:col-span-2">
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
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Risk profiles"
              :value="risk.length"
              accent="violet"
            />
          </div>
          <div class="col-span-6 md:col-span-2">
            <MetricCard
              label="Routes"
              :value="routes.length"
              accent="yellow"
              hint="OpenAPI"
            />
          </div>
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
            :value="cfg"
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
      </div>
    </template>
  </UDashboardPanel>
</template>
