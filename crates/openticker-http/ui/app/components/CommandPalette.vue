<script setup lang="ts">
import type { BotSummary } from '~/types/api'

const open = defineModel<boolean>('open', { default: false })

const router = useRouter()
const actions = useServiceActions()
const refresh = useAutoRefreshSettings()
const { api } = useApi()

const bots = shallowRef<BotSummary[]>([])
const loading = ref(false)

async function loadBots() {
  if (bots.value.length) return
  loading.value = true
  try {
    bots.value = await api<BotSummary[]>('/v1/bots?limit=500')
  } catch {
    /* silent */
  } finally {
    loading.value = false
  }
}

watch(open, (val) => {
  if (val) void loadBots()
})

interface CommandItem {
  label: string
  suffix?: string
  icon?: string
  kbds?: string[]
  onSelect: () => unknown
}

interface CommandGroup {
  id: string
  label?: string
  items: CommandItem[]
}

const pageItems = computed<CommandItem[]>(() => [
  { label: 'Overview', icon: 'i-lucide-layout-dashboard', onSelect: () => router.push('/') },
  { label: 'Bots', icon: 'i-lucide-bot', onSelect: () => router.push('/bots') },
  { label: 'Cycles', icon: 'i-lucide-git-branch', onSelect: () => router.push('/cycles') },
  { label: 'Portfolio', icon: 'i-lucide-pie-chart', onSelect: () => router.push('/portfolio') },
  { label: 'Feeds', icon: 'i-lucide-radio', onSelect: () => router.push('/feeds') },
  { label: 'Providers', icon: 'i-lucide-plug', onSelect: () => router.push('/providers') },
  { label: 'Connectors', icon: 'i-lucide-server', onSelect: () => router.push('/connectors') },
  { label: 'Activity', icon: 'i-lucide-activity', onSelect: () => router.push('/activity') },
  { label: 'Config', icon: 'i-lucide-sliders-horizontal', onSelect: () => router.push('/config') }
])

const actionItems = computed<CommandItem[]>(() => [
  {
    label: refresh.value.enabled ? 'Pause auto-refresh' : 'Resume auto-refresh',
    icon: refresh.value.enabled ? 'i-lucide-pause' : 'i-lucide-play',
    onSelect: () => {
      refresh.value.enabled = !refresh.value.enabled
    }
  },
  {
    label: 'Reload config',
    icon: 'i-lucide-rotate-ccw',
    onSelect: () => actions.reloadConfig()
  },
  {
    label: 'Engage kill switch',
    icon: 'i-lucide-shield-alert',
    onSelect: () => actions.killSwitchOn()
  },
  {
    label: 'Clear kill switch',
    icon: 'i-lucide-shield-check',
    onSelect: () => actions.killSwitchClear()
  }
])

const botItems = computed<CommandItem[]>(() =>
  bots.value.slice(0, 40).map((b) => ({
    label: b.display_name ?? b.id,
    suffix: (b.symbols ?? b.tickers ?? []).join(' · ') || (b.venue ?? ''),
    icon: 'i-lucide-bot',
    onSelect: () => router.push(`/bots/${b.id}`)
  }))
)

const search = ref('')

const groups = computed<CommandGroup[]>(() => {
  const q = search.value.trim().toLowerCase()
  const match = (it: CommandItem) => {
    if (!q) return true
    const hay = `${it.label} ${it.suffix ?? ''}`.toLowerCase()
    return hay.includes(q)
  }
  return [
    { id: 'pages', label: 'Pages', items: pageItems.value.filter(match) },
    { id: 'actions', label: 'Actions', items: actionItems.value.filter(match) },
    { id: 'bots', label: 'Bots', items: botItems.value.filter(match) }
  ].filter((g) => g.items.length)
})

function runItem(item: CommandItem) {
  open.value = false
  void Promise.resolve(item.onSelect())
}

defineShortcuts({
  meta_k: {
    usingInput: true,
    handler: () => {
      open.value = !open.value
    }
  },
  ctrl_k: {
    usingInput: true,
    handler: () => {
      open.value = !open.value
    }
  }
})
</script>

<template>
  <UModal
    v-model:open="open"
    :ui="{
      content: 'max-w-[640px] rounded-xl overflow-hidden',
      header: 'p-0',
      body: 'p-0',
      footer: 'border-t border-[color:var(--color-hairline)] px-4 py-2'
    }"
  >
    <template #header>
      <div class="flex items-center gap-2 px-4 py-3 border-b border-[color:var(--color-hairline)]">
        <UIcon
          name="i-lucide-search"
          class="size-4 text-ink-soft"
        />
        <input
          v-model="search"
          autofocus
          placeholder="Search pages, actions, bots..."
          class="flex-1 bg-transparent outline-none text-[14px] placeholder:text-ink-soft"
        />
        <kbd class="kbd">ESC</kbd>
      </div>
    </template>

    <template #body>
      <div class="max-h-[420px] overflow-y-auto py-1">
        <div
          v-if="loading"
          class="px-4 py-6 text-[12px] text-ink-soft font-data uppercase tracking-[0.14em] text-center"
        >
          Loading...
        </div>
        <div
          v-else-if="!groups.length"
          class="px-4 py-12 text-center"
        >
          <UIcon
            name="i-lucide-inbox"
            class="size-5 text-ink-soft mx-auto mb-2"
          />
          <div class="text-[13px] text-ink-soft">No matches.</div>
        </div>
        <div
          v-for="group in groups"
          :key="group.id"
          class="py-1"
        >
          <div class="px-4 py-1.5 text-[10px] uppercase tracking-[0.16em] text-ink-soft font-data">
            {{ group.label }}
          </div>
          <button
            v-for="item in group.items"
            :key="`${group.id}-${item.label}`"
            type="button"
            class="w-full flex items-center gap-3 px-4 py-2 text-left hover:bg-[color:var(--color-accent-softer)] transition-colors"
            @click="runItem(item)"
          >
            <UIcon
              v-if="item.icon"
              :name="item.icon"
              class="size-4 text-ink-soft"
            />
            <div class="flex-1 min-w-0">
              <div class="text-[13.5px] truncate">{{ item.label }}</div>
              <div
                v-if="item.suffix"
                class="text-[11px] text-ink-soft font-data truncate"
              >
                {{ item.suffix }}
              </div>
            </div>
            <UIcon
              name="i-lucide-corner-down-left"
              class="size-3.5 text-ink-soft opacity-0 group-hover:opacity-100"
            />
          </button>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-between text-[11px] text-ink-soft font-data uppercase tracking-[0.12em]">
        <div class="flex items-center gap-2">
          <kbd class="kbd">↵</kbd>
          <span>to run</span>
        </div>
        <div class="flex items-center gap-2">
          <kbd class="kbd">⌘</kbd>
          <kbd class="kbd">K</kbd>
          <span>anywhere</span>
        </div>
      </div>
    </template>
  </UModal>
</template>
