<script setup lang="ts">
import type { NavigationMenuItem } from '@nuxt/ui'
import type { ServiceStatus } from '~/types/api'

const { api } = useApi()

const status = shallowRef<ServiceStatus | null>(null)
const now = ref(new Date())

const loadStatus = async () => {
  try {
    status.value = await api<ServiceStatus>('/v1/service/status')
  } catch {
    /* silent */
  }
}

useAutoRefresh(loadStatus)
onMounted(() => {
  void loadStatus()
})
useIntervalFn(() => {
  now.value = new Date()
}, 1000)

const items = computed<NavigationMenuItem[][]>(() => [
  [
    {
      label: 'Overview',
      icon: 'i-lucide-layout-dashboard',
      to: '/'
    },
    {
      label: 'Bots',
      icon: 'i-lucide-bot',
      to: '/bots',
      badge: status.value?.running_instances != null ? String(status.value.running_instances) : undefined
    },
    {
      label: 'Cycles',
      icon: 'i-lucide-git-branch',
      to: '/cycles'
    },
    {
      label: 'Portfolio',
      icon: 'i-lucide-pie-chart',
      to: '/portfolio'
    }
  ],
  [
    {
      label: 'Feeds',
      icon: 'i-lucide-radio',
      to: '/feeds'
    },
    {
      label: 'Providers',
      icon: 'i-lucide-plug',
      to: '/providers'
    },
    {
      label: 'Connectors',
      icon: 'i-lucide-server',
      to: '/connectors'
    }
  ],
  [
    {
      label: 'Activity',
      icon: 'i-lucide-activity',
      to: '/activity'
    },
    {
      label: 'Config',
      icon: 'i-lucide-sliders-horizontal',
      to: '/config'
    }
  ]
])

const collapsedItems = computed<NavigationMenuItem[][]>(() =>
  items.value.map((group) => group.map(({ badge: _badge, ...rest }) => rest))
)

const killSwitchOn = computed(() => status.value?.kill_switch_active === true)
const ready = computed(() => status.value?.ready === true)
const utcClock = computed(() => {
  const d = now.value
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`
})

const paletteOpen = ref(false)
</script>

<template>
  <UDashboardGroup
    unit="px"
    :persistent="false"
    class="min-h-screen surface-canvas"
  >
    <UDashboardSidebar
      id="main"
      collapsible
      :default-size="264"
      :min-size="220"
      :max-size="320"
      :collapsed-size="64"
      :ui="{
        root: 'overflow-x-hidden data-[collapsed=false]:!w-[264px] data-[collapsed=true]:!w-16 data-[collapsed=true]:!min-w-16',
        header: 'data-[collapsed=true]:px-0 data-[collapsed=true]:justify-center',
        body: 'data-[collapsed=true]:px-0',
        footer: 'data-[collapsed=true]:px-0 data-[collapsed=true]:justify-center'
      }"
      class="border-r border-[color:var(--color-hairline)] bg-white"
    >
      <template #header="{ collapsed, collapse }">
        <NuxtLink
          to="/"
          class="flex items-center gap-2.5 min-w-0 select-none flex-1"
          :class="collapsed ? 'justify-center' : ''"
        >
          <div
            class="w-7 h-7 shrink-0 rounded-md grid place-items-center font-editorial text-[18px] leading-none text-white italic"
            style="background: linear-gradient(135deg, var(--color-accent) 0%, var(--color-accent-strong) 100%)"
          >
            O
          </div>
          <div
            v-if="!collapsed"
            class="leading-tight min-w-0"
          >
            <div class="font-editorial text-[22px] tracking-tight text-ink truncate">OpenTicker</div>
            <div class="text-[10px] uppercase tracking-[0.16em] text-ink-soft font-data truncate">
              Control Workspace
            </div>
          </div>
        </NuxtLink>
      </template>

      <template #default="{ collapsed }">
        <UNavigationMenu
          :items="collapsed ? collapsedItems : items"
          orientation="vertical"
          :collapsed="collapsed"
          :tooltip="collapsed"
          :ui="{
            link: 'rounded-md text-[13.5px] tracking-tight data-[active=true]:bg-[color:var(--color-accent-soft)] data-[active=true]:text-[color:var(--color-accent-ink)] hover:bg-[color:var(--color-canvas)]',
            linkLeadingIcon: 'size-4 data-[active=true]:text-[color:var(--color-accent-strong)]',
            linkLabel: 'font-medium',
            linkTrailingBadge: 'pastel-accent text-[10px] rounded-full',
            childList: 'mt-0'
          }"
        />

        <button
          v-if="!collapsed"
          type="button"
          class="mt-3 mx-2 w-[calc(100%-1rem)] flex items-center gap-2 px-3 py-2 rounded-md border border-[color:var(--color-hairline)] bg-white hover:border-[color:var(--color-hairline-strong)] transition-colors group"
          @click="paletteOpen = true"
        >
          <UIcon
            name="i-lucide-search"
            class="size-3.5 text-ink-soft"
          />
          <span class="text-[12px] text-ink-soft flex-1 text-left">Quick find</span>
          <kbd class="kbd text-[10px]">⌘K</kbd>
        </button>

        <div
          v-if="!collapsed"
          class="mt-5 px-2 space-y-3"
        >
          <div class="text-[10px] uppercase tracking-[0.16em] text-ink-soft font-data px-2">Runtime</div>
          <div class="grid grid-cols-2 gap-1.5 px-1">
            <div class="hairline rounded-md px-3 py-2 bg-white">
              <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft">Ready</div>
              <div class="mt-0.5 flex items-center gap-1.5 text-[13px] font-medium">
                <span
                  class="dot"
                  :class="ready ? 'bg-[color:var(--color-pastel-green-ink)]' : 'bg-[color:var(--color-pastel-red-ink)]'"
                />
                {{ ready ? 'Yes' : 'No' }}
              </div>
            </div>
            <div class="hairline rounded-md px-3 py-2 bg-white">
              <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft">Mode</div>
              <div class="mt-0.5 text-[13px] font-medium truncate">
                {{ status?.mode_banner ?? '—' }}
              </div>
            </div>
            <div class="hairline rounded-md px-3 py-2 bg-white">
              <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft">Bots</div>
              <div class="mt-0.5 text-[13px] font-medium font-data tabular-nums">
                {{ status?.total_instances ?? '—' }}
              </div>
            </div>
            <div class="hairline rounded-md px-3 py-2 bg-white">
              <div class="text-[10px] uppercase tracking-[0.14em] text-ink-soft">Running</div>
              <div class="mt-0.5 text-[13px] font-medium font-data tabular-nums">
                {{ status?.running_instances ?? '—' }}
              </div>
            </div>
          </div>
          <div
            v-if="killSwitchOn"
            class="mx-1 rounded-md pastel-red px-3 py-2 text-[11px] tracking-tight flex items-center gap-2"
          >
            <UIcon
              name="i-lucide-shield-alert"
              class="size-3.5"
            />
            Kill switch engaged
          </div>
        </div>
      </template>

      <template #footer="{ collapsed, collapse }">
        <div
          v-if="!collapsed"
          class="text-[10px] uppercase tracking-[0.16em] text-ink-soft font-data text-center py-1 flex-1"
        >
          {{ utcClock }}
        </div>
        <UButton
          v-else
          color="neutral"
          variant="ghost"
          size="sm"
          icon="i-lucide-panel-left-open"
          aria-label="Expand sidebar"
          @click="collapse(false)"
        />
      </template>
    </UDashboardSidebar>

    <slot />

    <CommandPalette v-model:open="paletteOpen" />
  </UDashboardGroup>
</template>
