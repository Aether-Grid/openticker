<script setup lang="ts">
const props = defineProps<{
  value: unknown
  maxHeight?: string
}>()

const formatted = computed(() => {
  try {
    return JSON.stringify(props.value, null, 2)
  } catch {
    return String(props.value)
  }
})
</script>

<template>
  <pre
    class="surface-card font-data text-[11.5px] leading-relaxed text-ink overflow-auto p-4 tabular-nums whitespace-pre"
    :style="{ maxHeight: maxHeight ?? '320px' }"
    >{{ formatted }}</pre
  >
</template>
