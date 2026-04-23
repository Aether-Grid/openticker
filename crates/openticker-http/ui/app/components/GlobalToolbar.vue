<script setup lang="ts">
import type { ServiceStatus } from '~/types/api'

const props = defineProps<{
  status?: ServiceStatus | null
  lastUpdated?: Date | null
  pending?: boolean
}>()

const emit = defineEmits<{
  refresh: []
}>()

const actions = useServiceActions()
const refresh = useAutoRefreshSettings()

const autoLabel = computed(() =>
  refresh.value.enabled ? `Auto · ${(refresh.value.intervalMs / 1000).toFixed(0)}s` : 'Auto off'
)
const lastLabel = computed(() => {
  if (!props.lastUpdated) return 'Not yet'
  const d = props.lastUpdated
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
})
const killOn = computed(() => props.status?.kill_switch_active === true)
</script>

<template>
  <div class="flex items-center gap-2 flex-wrap">
    <div class="hidden md:flex items-center gap-3 text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data pr-2">
      <span class="flex items-center gap-1.5">
        <span
          class="dot"
          :class="
            status?.ready ? 'bg-[color:var(--color-pastel-green-ink)]' : 'bg-[color:var(--color-pastel-yellow-ink)]'
          "
        />
        {{ status?.ready ? 'Ready' : 'Init' }}
      </span>
      <span class="text-ink-soft">Updated {{ lastLabel }}</span>
    </div>

    <UButton
      size="xs"
      color="neutral"
      variant="outline"
      :icon="refresh.enabled ? 'i-lucide-pause' : 'i-lucide-play'"
      :label="autoLabel"
      @click="refresh.enabled = !refresh.enabled"
    />

    <UButton
      size="xs"
      color="neutral"
      variant="outline"
      icon="i-lucide-rotate-cw"
      label="Refresh"
      :loading="pending"
      @click="emit('refresh')"
    />

    <UButton
      size="xs"
      color="neutral"
      variant="outline"
      icon="i-lucide-rotate-ccw"
      label="Reload config"
      @click="actions.reloadConfig()"
    />

    <UButton
      size="xs"
      :color="killOn ? 'neutral' : 'error'"
      :variant="killOn ? 'subtle' : 'outline'"
      :icon="killOn ? 'i-lucide-shield-check' : 'i-lucide-shield-alert'"
      :label="killOn ? 'Clear kill switch' : 'Kill switch'"
      @click="killOn ? actions.killSwitchClear() : actions.killSwitchOn()"
    />
  </div>
</template>
