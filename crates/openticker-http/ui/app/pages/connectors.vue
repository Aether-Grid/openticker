<script setup lang="ts">
import type { ConnectorMatrixEntry, ConnectorStatus } from '~/types/api'
import { fmtRelative, short } from '~/utils/format'

definePageMeta({ layout: 'default' })

const { api } = useApi()
const statuses = shallowRef<ConnectorStatus[]>([])
const matrix = shallowRef<ConnectorMatrixEntry[]>([])
const pending = ref(false)
const lastLoadedAt = ref<Date | null>(null)
const selectedId = ref<string | null>(null)

const load = async () => {
  pending.value = true
  try {
    const [s, m] = await Promise.all([
      api<ConnectorStatus[]>('/v1/connectors/status').catch(() => [] as ConnectorStatus[]),
      api<ConnectorMatrixEntry[] | Record<string, ConnectorMatrixEntry>>('/v1/connectors/matrix').catch(
        () => [] as ConnectorMatrixEntry[]
      )
    ])
    statuses.value = s
    matrix.value = Array.isArray(m) ? m : Object.values(m ?? {})
    lastLoadedAt.value = new Date()
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
onMounted(() => {
  void load()
})

watch(statuses, (list) => {
  if (!selectedId.value && list.length) selectedId.value = list[0]?.account ?? null
})

const selected = computed(() => statuses.value.find((s) => s.account === selectedId.value) ?? null)

const stateCounts = computed(() => {
  const buckets = { connected: 0, degraded: 0, disconnected: 0 }
  statuses.value.forEach((c) => {
    const s = (c.state ?? '').toLowerCase()
    if (s.includes('connect')) buckets.connected++
    else if (s.includes('degrad')) buckets.degraded++
    else buckets.disconnected++
  })
  return buckets
})

const resilienceWindows = computed(() =>
  statuses.value.reduce((acc, c) => {
    const r = c.resilience
    if (typeof r === 'string') return acc
    if (r && typeof r === 'object') return acc + Object.keys(r).length
    return acc
  }, 0)
)

const selectedUpdatedAt = computed(() => {
  const raw = (selected.value as Record<string, unknown> | null)?.updated_at
  return typeof raw === 'string' ? raw : null
})

function stateTone(state?: string): 'green' | 'yellow' | 'red' | 'neutral' {
  const s = (state ?? '').toLowerCase()
  if (s.includes('connect')) return 'green'
  if (s.includes('degrad')) return 'yellow'
  return 'red'
}

const tableCols = [
  { key: 'account', label: 'Account' },
  { key: 'kind', label: 'Kind' },
  { key: 'mode', label: 'Mode' },
  { key: 'state', label: 'State' },
  { key: 'resilience', label: 'Resilience' },
  { key: 'message', label: 'Message' }
]
</script>

<template>
  <UDashboardPanel>
    <template #header>
      <UDashboardNavbar :ui="{ root: 'border-b border-[color:var(--color-hairline)] px-8 h-14' }">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #title>
          <span class="font-data text-[11px] uppercase tracking-[0.18em] text-ink-soft">Connectors</span>
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
          eyebrow="Connectors"
          title="Bridges between bots and markets."
          description="Live state for each execution and data adapter, plus the capability matrix driving what's possible."
        />

        <section class="grid grid-cols-2 md:grid-cols-5 gap-3">
          <MetricCard
            label="Total"
            :value="statuses.length"
          />
          <MetricCard
            label="Connected"
            :value="stateCounts.connected"
            accent="green"
          />
          <MetricCard
            label="Degraded"
            :value="stateCounts.degraded"
            :accent="stateCounts.degraded ? 'yellow' : 'neutral'"
          />
          <MetricCard
            label="Disconnected"
            :value="stateCounts.disconnected"
            :accent="stateCounts.disconnected ? 'red' : 'neutral'"
          />
          <MetricCard
            label="Resilience"
            :value="resilienceWindows"
            accent="blue"
            hint="Active windows"
          />
        </section>

        <section class="grid grid-cols-12 gap-8">
          <div class="col-span-12 xl:col-span-7 space-y-4">
            <SectionHeader
              label="Runtime status"
              hint="Click a row to open details."
            />
            <div class="surface-card overflow-hidden">
              <div class="overflow-x-auto">
                <table class="w-full text-[13px] border-collapse">
                  <thead>
                    <tr class="text-left">
                      <th
                        v-for="col in tableCols"
                        :key="col.key"
                        class="px-4 py-2.5 text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data font-medium hairline-b"
                      >
                        {{ col.label }}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="c in statuses"
                      :key="c.account"
                      class="hairline-b last:border-b-0 hover:bg-[color:var(--color-canvas)] transition-colors cursor-pointer"
                      :class="[selectedId === c.account ? 'bg-[color:var(--color-canvas)]' : '']"
                      @click="selectedId = c.account"
                    >
                      <td class="px-4 py-3 font-editorial text-[15px]">
                        {{ c.account }}
                      </td>
                      <td class="px-4 py-3 font-data text-[12.5px]">
                        {{ c.kind ?? '—' }}
                      </td>
                      <td class="px-4 py-3 font-data text-[12.5px]">
                        {{ c.mode ?? '—' }}
                      </td>
                      <td class="px-4 py-3">
                        <StatusPill
                          :label="c.state ?? '—'"
                          :tone="stateTone(c.state)"
                        />
                      </td>
                      <td class="px-4 py-3 font-data text-[12.5px]">
                        {{
                          typeof c.resilience === 'string'
                            ? c.resilience
                            : c.resilience && typeof c.resilience === 'object'
                              ? Object.keys(c.resilience).length + ' windows'
                              : '—'
                        }}
                      </td>
                      <td class="px-4 py-3 font-data text-[12px] text-ink-soft">
                        {{ short(c.message ?? '', 40) || '—' }}
                      </td>
                    </tr>
                    <tr v-if="!statuses.length">
                      <td
                        :colspan="tableCols.length"
                        class="px-6 py-12 text-center text-ink-soft"
                      >
                        No connectors registered.
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            <SectionHeader
              label="Capability matrix"
              hint="What each connector kind can do."
            />
            <div
              v-if="!matrix.length"
              class="text-[13px] text-ink-soft"
            >
              Matrix unavailable.
            </div>
            <div
              v-else
              class="grid grid-cols-1 md:grid-cols-2 gap-3"
            >
              <div
                v-for="entry in matrix"
                :key="entry.kind"
                class="surface-card p-5"
              >
                <div class="flex items-center justify-between">
                  <div class="font-editorial text-[18px]">
                    {{ entry.kind }}
                  </div>
                  <StatusPill
                    :label="`${entry.capabilities?.length ?? 0} caps`"
                    tone="neutral"
                  />
                </div>
                <div class="mt-3 flex flex-wrap gap-1.5">
                  <span
                    v-for="cap in entry.capabilities ?? []"
                    :key="cap"
                    class="pastel-blue text-[10.5px] uppercase tracking-[0.06em] rounded-full px-2 py-0.5 font-data"
                    >{{ cap }}</span
                  >
                </div>
                <div
                  v-if="entry.transports?.length"
                  class="mt-2 text-[11px] text-ink-soft font-data truncate"
                >
                  {{ entry.transports.join(' · ') }}
                </div>
              </div>
            </div>
          </div>

          <div class="col-span-12 xl:col-span-5 space-y-4">
            <SectionHeader
              label="Inspector"
              hint="Runtime detail for selected connector."
            />
            <div
              v-if="selected"
              class="space-y-3"
            >
              <div class="surface-card p-5 grid grid-cols-2 gap-3 text-[12.5px]">
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">State</div>
                  <div class="mt-1">
                    <StatusPill
                      :label="selected.state ?? '—'"
                      :tone="stateTone(selected.state)"
                    />
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Kind</div>
                  <div class="mt-1 font-data">
                    {{ selected.kind ?? '—' }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Mode</div>
                  <div class="mt-1 font-data">
                    {{ selected.mode ?? '—' }}
                  </div>
                </div>
                <div>
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Updated</div>
                  <div class="mt-1 font-data">
                    {{ fmtRelative(selectedUpdatedAt) }}
                  </div>
                </div>
                <div class="col-span-2">
                  <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Message</div>
                  <div class="mt-1 text-[12.5px]">
                    {{ selected.message ?? '—' }}
                  </div>
                </div>
              </div>
              <JsonBlock :value="selected" />
            </div>
            <EmptyState
              v-else
              icon="i-lucide-plug"
              title="Select a connector"
            />
          </div>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
