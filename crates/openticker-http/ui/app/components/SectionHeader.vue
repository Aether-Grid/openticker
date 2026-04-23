<script setup lang="ts">
defineProps<{
  label: string
  hint?: string
}>()
</script>

<template>
  <div class="flex items-end justify-between gap-4 pb-3">
    <div class="min-w-0">
      <div class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">
        {{ label }}
      </div>
      <p
        v-if="hint"
        class="text-[13px] text-ink-soft mt-1"
      >
        {{ hint }}
      </p>
    </div>
    <div class="shrink-0 flex items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
