<script setup lang="ts">
const props = defineProps<{
  label: string
  value: string | number
  hint?: string
  accent?: 'neutral' | 'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'orange' | 'teal' | 'accent'
  icon?: string
  compact?: boolean
}>()

const accentInk: Record<string, string> = {
  green: 'var(--color-pastel-green-ink)',
  red: 'var(--color-pastel-red-ink)',
  blue: 'var(--color-pastel-blue-ink)',
  yellow: 'var(--color-pastel-yellow-ink)',
  violet: 'var(--color-pastel-violet-ink)',
  orange: 'var(--color-pastel-orange-ink)',
  teal: 'var(--color-pastel-teal-ink)',
  accent: 'var(--color-accent-strong)'
}

const accentBorder = computed(() => {
  if (!props.accent || props.accent === 'neutral') return 'transparent'
  return accentInk[props.accent] ?? 'transparent'
})

const accentBgClass: Record<string, string> = {
  green: 'pastel-green',
  red: 'pastel-red',
  blue: 'pastel-blue',
  yellow: 'pastel-yellow',
  violet: 'pastel-violet',
  orange: 'pastel-orange',
  teal: 'pastel-teal',
  accent: 'pastel-accent'
}
</script>

<template>
  <div
    class="surface-card flex flex-col justify-between gap-3 transition-all duration-200 relative overflow-hidden"
    :class="compact ? 'p-4' : 'p-5'"
    style="min-height: 112px"
  >
    <div
      v-if="accent && accent !== 'neutral'"
      class="absolute top-0 left-0 bottom-0 w-[3px]"
      :style="{ backgroundColor: accentBorder }"
    />
    <div class="flex items-start justify-between gap-3">
      <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">
        {{ label }}
      </div>
      <div
        v-if="accent && accent !== 'neutral'"
        class="rounded-full px-1.5 h-5 grid place-items-center"
        :class="accentBgClass[accent]"
      >
        <UIcon
          v-if="icon"
          :name="icon"
          class="size-3"
        />
        <span
          v-else
          class="w-1.5 h-1.5 rounded-full"
          :style="{ backgroundColor: accentBorder }"
        />
      </div>
    </div>
    <div>
      <div
        class="font-data tabular-nums text-ink"
        :class="compact ? 'text-[22px]' : 'text-[28px]'"
        style="line-height: 1.05; letter-spacing: -0.02em"
      >
        {{ value }}
      </div>
      <div
        v-if="hint"
        class="mt-1 text-[11px] text-ink-soft truncate"
      >
        {{ hint }}
      </div>
    </div>
  </div>
</template>
