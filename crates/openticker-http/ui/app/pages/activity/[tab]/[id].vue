<script setup lang="ts">
import { extractItems, fmtDateTimeMs, fmtNumber, fmtRelativeMs, short, tryParseJson } from '~/utils/format'

definePageMeta({ layout: 'default' })

type TabKey = 'events' | 'signals' | 'intents' | 'risk' | 'orders' | 'fills' | 'positions' | 'reconciliations'
type ActivityRow = Record<string, unknown>

const ALLOWED_TABS: TabKey[] = [
  'events',
  'signals',
  'intents',
  'risk',
  'orders',
  'fills',
  'positions',
  'reconciliations'
]

const TAB_LABEL: Record<TabKey, string> = {
  events: 'Event',
  signals: 'Signal',
  intents: 'Intent',
  risk: 'Risk decision',
  orders: 'Order',
  fills: 'Fill',
  positions: 'Position',
  reconciliations: 'Reconciliation'
}

const TAB_ENDPOINT: Record<TabKey, string> = {
  events: '/v1/events',
  signals: '/v1/signals',
  intents: '/v1/intents',
  risk: '/v1/risk-decisions',
  orders: '/v1/orders',
  fills: '/v1/fills',
  positions: '/v1/positions',
  reconciliations: '/v1/reconciliations'
}

const route = useRoute()
const router = useRouter()
const { api } = useApi()

const rawTab = computed(() => route.params.tab as string)
const rawId = computed(() => route.params.id as string)
const tab = computed<TabKey>(() =>
  ALLOWED_TABS.includes(rawTab.value as TabKey) ? (rawTab.value as TabKey) : 'signals'
)
const recordId = computed(() => Number(rawId.value))
const tabLabel = computed(() => TAB_LABEL[tab.value])

const records = shallowRef<ActivityRow[]>([])
const pending = ref(false)
const error = ref<string | null>(null)
const lastLoadedAt = ref<Date | null>(null)

async function load() {
  pending.value = true
  error.value = null
  try {
    const endpoint = TAB_ENDPOINT[tab.value]
    const raw = await api<unknown>(`${endpoint}?limit=1000`)
    const list = Array.isArray(raw)
      ? (raw as ActivityRow[])
      : extractItems<ActivityRow>(raw as { items?: ActivityRow[] } | ActivityRow[])
    records.value = list
      .filter((r): r is ActivityRow => !!r && typeof r === 'object')
      .sort((a, b) => ((b.created_at_ms as number) ?? 0) - ((a.created_at_ms as number) ?? 0))
    lastLoadedAt.value = new Date()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load record.'
    records.value = []
  } finally {
    pending.value = false
  }
}

useAutoRefresh(load)
watch(
  [tab, recordId],
  () => {
    void load()
  },
  { immediate: true }
)

const record = computed<ActivityRow | null>(
  () => records.value.find((r) => (r.id as number) === recordId.value) ?? null
)

const recordIndex = computed(() => records.value.findIndex((r) => (r.id as number) === recordId.value))
const prevRecord = computed<ActivityRow | null>(() => {
  const idx = recordIndex.value
  return idx > 0 ? (records.value[idx - 1] ?? null) : null
})
const nextRecord = computed<ActivityRow | null>(() => {
  const idx = recordIndex.value
  return idx >= 0 && idx < records.value.length - 1 ? (records.value[idx + 1] ?? null) : null
})

function recordBotId(r: ActivityRow | null | undefined): string | undefined {
  if (!r) return undefined
  if (typeof r.bot_id === 'string') return r.bot_id
  if (typeof r.entity_id === 'string' && r.scope === 'bot') return r.entity_id
  return undefined
}

function labelize(value: unknown): string {
  if (value === null || value === undefined) return '—'
  return String(value).replace(/_/g, ' ')
}

function valueTone(key: string, val: unknown): 'green' | 'red' | 'yellow' | 'blue' | 'violet' | 'accent' | 'neutral' {
  if (typeof val === 'boolean') return val ? 'green' : 'neutral'
  if (typeof val !== 'string') return 'neutral'
  const v = val.toLowerCase()
  if (key === 'signal') {
    if (v.startsWith('buy')) return 'green'
    if (v.startsWith('sell')) return 'red'
    return 'violet'
  }
  if (key === 'intent') {
    if (v === 'open_long' || v === 'add_long') return 'green'
    if (v === 'close_long' || v === 'reduce_long') return 'red'
    return 'blue'
  }
  if (key === 'decision') return v === 'allowed' ? 'green' : 'red'
  if (key === 'status') {
    if (v.includes('fill')) return 'green'
    if (v.includes('reject') || v.includes('cancel') || v.includes('expired')) return 'red'
    if (v.includes('partial')) return 'yellow'
    if (v.includes('open') || v.includes('working') || v.includes('pending') || v.includes('new')) return 'blue'
    return 'neutral'
  }
  if (key === 'phase') return v === 'confirmed' ? 'accent' : 'neutral'
  if (key === 'scope') {
    if (v === 'bot') return 'accent'
    if (v === 'service') return 'violet'
    if (v === 'connector') return 'blue'
    return 'neutral'
  }
  return 'neutral'
}

const EXCLUDE_KEYS = new Set([
  'id',
  'bot_id',
  'entity_id',
  'symbol',
  'trace_id',
  'created_at_ms',
  'payload',
  'metadata_json',
  'scope',
  'kind'
])

type FieldEntry = { key: string; value: unknown }

const fields = computed<FieldEntry[]>(() => {
  const r = record.value
  if (!r) return []
  return Object.entries(r)
    .filter(([k]) => !EXCLUDE_KEYS.has(k))
    .map(([key, value]) => ({ key, value }))
})

const pillFields = computed(() =>
  fields.value.filter(({ value }) => typeof value === 'boolean' || typeof value === 'string')
)
const numericFields = computed(() => fields.value.filter(({ value }) => typeof value === 'number'))

const parsedPayload = computed<unknown>(() => {
  const r = record.value
  if (!r) return null
  if (tab.value === 'events' && typeof r.payload === 'string') {
    return tryParseJson(r.payload)
  }
  if (typeof r.metadata_json === 'string') {
    return tryParseJson(r.metadata_json)
  }
  return null
})

const headerTitle = computed(() => {
  const r = record.value
  if (!r) return `${tabLabel.value} #${recordId.value}`
  if (typeof r.symbol === 'string') return r.symbol
  if (typeof r.kind === 'string') return r.kind
  return `${tabLabel.value} #${recordId.value}`
})

const headerEyebrow = computed(() => {
  const r = record.value
  const bot = recordBotId(r)
  if (bot) return `${tabLabel.value} · ${bot}`
  if (r && typeof r.scope === 'string') return `${tabLabel.value} · ${r.scope}`
  return tabLabel.value
})

function goToSibling(next: ActivityRow | null) {
  if (!next) return
  void router.push(`/activity/${tab.value}/${next.id as number}`)
}

function backToList() {
  void router.push({ path: '/activity', query: { tab: tab.value } })
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
              :to="{ path: '/activity', query: { tab } }"
              class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data hover:text-ink transition-colors"
            >
              Activity /
            </NuxtLink>
            <span class="text-[11px] uppercase tracking-[0.18em] text-ink-soft font-data capitalize">{{ tab }} /</span>
            <span class="font-data text-[13px]">#{{ recordId }}</span>
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
      <div class="px-8 py-10 max-w-[1200px] mx-auto w-full space-y-8">
        <PageHeader
          :eyebrow="headerEyebrow"
          :title="headerTitle"
          :description="
            record ? `Record #${recordId} · ${fmtDateTimeMs(record.created_at_ms as number)}` : 'Locating record...'
          "
        >
          <template #actions>
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-arrow-left"
              label="Back"
              @click="backToList"
            />
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-chevron-left"
              :disabled="!prevRecord"
              aria-label="Newer record"
              @click="goToSibling(prevRecord)"
            />
            <UButton
              size="xs"
              color="neutral"
              variant="outline"
              icon="i-lucide-chevron-right"
              :disabled="!nextRecord"
              aria-label="Older record"
              @click="goToSibling(nextRecord)"
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
            <div class="font-medium">Could not load record.</div>
            <div class="text-ink-soft mt-0.5">{{ error }}</div>
          </div>
        </div>

        <EmptyState
          v-else-if="!pending && !record"
          icon="i-lucide-search-x"
          title="Record not found"
          description="It may have been pruned from the recent window. Try widening the list limit on the activity page."
        />

        <template v-else-if="record">
          <section class="surface-card p-6 space-y-5">
            <div class="grid grid-cols-1 md:grid-cols-3 gap-5">
              <div>
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Bot</div>
                <div class="mt-1 font-data text-[13.5px]">
                  {{ recordBotId(record) ?? '—' }}
                </div>
              </div>
              <div v-if="typeof record.symbol === 'string'">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Symbol</div>
                <div class="mt-1 font-editorial text-[18px]">
                  {{ record.symbol }}
                </div>
              </div>
              <div v-if="tab === 'events' && typeof record.kind === 'string'">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Kind</div>
                <div class="mt-1 font-data text-[13.5px]">
                  {{ record.kind }}
                </div>
              </div>
              <div v-if="tab === 'events' && typeof record.scope === 'string'">
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Scope</div>
                <div class="mt-1">
                  <StatusPill
                    :label="record.scope as string"
                    :tone="valueTone('scope', record.scope)"
                  />
                </div>
              </div>
              <div>
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Recorded</div>
                <div class="mt-1 font-data text-[13.5px]">
                  {{ fmtRelativeMs(record.created_at_ms as number) }}
                </div>
                <div class="text-[10.5px] text-ink-soft font-data mt-0.5">
                  {{ fmtDateTimeMs(record.created_at_ms as number) }}
                </div>
              </div>
              <div>
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Record id</div>
                <div class="mt-1 font-data tabular-nums text-[13.5px]">#{{ record.id }}</div>
              </div>
            </div>

            <div
              v-if="typeof record.trace_id === 'string' && record.trace_id"
              class="pt-4 hairline-t"
            >
              <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Trace</div>
              <NuxtLink
                v-if="recordBotId(record)"
                :to="`/cycles/${encodeURIComponent(recordBotId(record)!)}/${encodeURIComponent(record.trace_id as string)}`"
                class="mt-1 inline-flex items-center gap-1.5 text-[13px] font-data text-[color:var(--color-accent-strong)] hover:underline truncate max-w-full"
              >
                <UIcon
                  name="i-lucide-link-2"
                  class="size-3.5"
                />
                {{ record.trace_id }}
              </NuxtLink>
              <div
                v-else
                class="mt-1 text-[13px] font-data truncate"
              >
                {{ record.trace_id }}
              </div>
            </div>
          </section>

          <section
            v-if="pillFields.length"
            class="space-y-3"
          >
            <SectionHeader label="Attributes" />
            <div class="surface-card p-5 grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
              <div
                v-for="{ key, value } in pillFields"
                :key="key"
                class="min-w-0"
              >
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">
                  {{ labelize(key) }}
                </div>
                <div class="mt-1.5 text-[13px]">
                  <template v-if="typeof value === 'boolean'">
                    <StatusPill
                      :label="value ? 'yes' : 'no'"
                      :tone="value ? 'green' : 'neutral'"
                    />
                  </template>
                  <template v-else-if="typeof value === 'string' && value.length && value.length <= 24">
                    <StatusPill
                      :label="labelize(value)"
                      :tone="valueTone(key, value)"
                    />
                  </template>
                  <template v-else-if="typeof value === 'string' && value">
                    <span class="font-data text-ink">{{ value }}</span>
                  </template>
                  <template v-else>
                    <span class="text-ink-soft">—</span>
                  </template>
                </div>
              </div>
            </div>
          </section>

          <section
            v-if="numericFields.length"
            class="space-y-3"
          >
            <SectionHeader label="Numbers" />
            <div class="surface-card p-5 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
              <div
                v-for="{ key, value } in numericFields"
                :key="key"
                class="min-w-0"
              >
                <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">
                  {{ labelize(key) }}
                </div>
                <div class="mt-1 font-data tabular-nums text-[14px]">
                  {{ fmtNumber(value as number, key.includes('price') || key.includes('quantity') ? 4 : 2) }}
                </div>
              </div>
            </div>
          </section>

          <section
            v-if="tab === 'events' && typeof record.payload === 'string' && record.payload"
            class="space-y-3"
          >
            <SectionHeader
              label="Payload"
              :hint="parsedPayload && typeof parsedPayload === 'object' ? 'Decoded JSON.' : 'Raw string.'"
            />
            <JsonBlock
              v-if="parsedPayload && typeof parsedPayload === 'object'"
              :value="parsedPayload"
              max-height="480px"
            />
            <pre
              v-else
              class="surface-card font-data text-[12px] leading-relaxed text-ink overflow-auto p-4 whitespace-pre-wrap"
              >{{ short(record.payload as string, 4000) }}</pre
            >
          </section>

          <section
            v-else-if="parsedPayload && typeof parsedPayload === 'object'"
            class="space-y-3"
          >
            <SectionHeader
              label="Parsed metadata"
              hint="Decoded from metadata_json."
            />
            <JsonBlock
              :value="parsedPayload"
              max-height="480px"
            />
          </section>

          <section class="space-y-3">
            <SectionHeader
              label="Raw record"
              hint="Complete JSON payload from the API."
            />
            <JsonBlock
              :value="record"
              max-height="520px"
            />
          </section>
        </template>
      </div>
    </template>
  </UDashboardPanel>
</template>
