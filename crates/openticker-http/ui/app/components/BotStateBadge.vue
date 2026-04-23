<script setup lang="ts">
const props = defineProps<{
  state?: string | null
}>()

const tone = computed<'green' | 'red' | 'blue' | 'yellow' | 'violet' | 'neutral'>(() => {
  const s = (props.state ?? '').toLowerCase()
  if (s.includes('run')) return 'green'
  if (s.includes('paus')) return 'yellow'
  if (s.includes('reconcil')) return 'blue'
  if (s.includes('stop') || s.includes('disabled')) return 'neutral'
  if (s.includes('block') || s.includes('err')) return 'red'
  return 'neutral'
})
const label = computed(() => (props.state ? props.state.replace(/_/g, ' ') : 'unknown'))
</script>

<template>
  <StatusPill
    :label="label"
    :tone="tone"
  />
</template>
