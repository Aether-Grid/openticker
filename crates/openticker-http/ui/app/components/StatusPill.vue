<script setup lang="ts">
const props = defineProps<{
  label: string
  tone?: 'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'orange' | 'teal' | 'accent' | 'neutral'
}>()

const toneClass = computed(() => {
  switch (props.tone) {
    case 'green':
      return 'pastel-green'
    case 'red':
      return 'pastel-red'
    case 'blue':
      return 'pastel-blue'
    case 'yellow':
      return 'pastel-yellow'
    case 'violet':
      return 'pastel-violet'
    case 'orange':
      return 'pastel-orange'
    case 'teal':
      return 'pastel-teal'
    case 'accent':
      return 'pastel-accent'
    default:
      return 'bg-[color:var(--color-canvas)] text-ink border border-[color:var(--color-hairline)]'
  }
})
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.05em] leading-none"
    :class="toneClass"
  >
    <slot name="before" />
    {{ label }}
    <slot />
  </span>
</template>
