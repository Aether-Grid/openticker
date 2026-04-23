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

const items = computed<NavigationMenuItem[]>(() => [
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

const killSwitchOn = computed(() => status.value?.kill_switch_active === true)
const ready = computed(() => status.value?.ready === true)
const utcClock = computed(() => {
  const d = now.value
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`
})
</script>

<template>
  <UDashboardGroup
    unit="px"
    class="min-h-screen surface-canvas"
  >
    <UDashboardSidebar
      collapsible
      resizable
      class="border-r border-[color:var(--color-hairline)] bg-white"
    >
      <template #header="{ collapsed }">
        <NuxtLink
          to="/"
          class="flex items-center gap-2.5 px-1 py-1 select-none"
        >
          <div
            class="w-7 h-7 rounded-md grid place-items-center font-editorial text-[15px] leading-none text-white"
            style="background: linear-gradient(135deg, var(--color-accent) 0%, var(--color-accent-strong) 100%)"
          >
            O
          </div>
          <div
            v-if="!collapsed"
            class="leading-tight"
          >
            <div class="font-editorial text-[17px] tracking-tight text-ink">OpenTicker</div>
            <div class="text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data">Control Workspace</div>
          </div>
        </NuxtLink>
      </template>

      <template #default="{ collapsed }">
        <UNavigationMenu
          :items="items"
          orientation="vertical"
          :collapsed="collapsed"
          :ui="{
            link: 'rounded-md text-[13.5px] tracking-tight data-[active=true]:bg-[color:var(--color-accent-soft)] data-[active=true]:text-[color:var(--color-accent-ink)] hover:bg-[color:var(--color-canvas)]',
            linkLeadingIcon: 'size-4 data-[active=true]:text-[color:var(--color-accent-strong)]',
            linkLabel: 'font-medium',
            linkTrailingBadge: 'pastel-accent text-[10px] rounded-full',
            childList: 'mt-0'
          }"
        />

        <div
          v-if="!collapsed"
          class="mt-6 px-2 space-y-3"
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

      <template #footer="{ collapsed }">
        <div
          v-if="!collapsed"
          class="text-[10px] uppercase tracking-[0.16em] text-ink-soft font-data text-center py-1"
        >
          {{ utcClock }}
        </div>
        <UDashboardSidebarCollapse v-else />
      </template>
    </UDashboardSidebar>

    <slot />
  </UDashboardGroup>
</template>
