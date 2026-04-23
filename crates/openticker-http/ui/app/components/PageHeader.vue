<script setup lang="ts">
defineProps<{
  eyebrow?: string
  title: string
  description?: string
  meta?: string
}>()
</script>

<template>
  <div class="flex items-end justify-between gap-6 pb-6">
    <div class="space-y-1.5 min-w-0">
      <div
        v-if="eyebrow"
        class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data"
      >
        {{ eyebrow }}
      </div>
      <h1 class="font-editorial text-[38px] md:text-[44px] text-ink truncate">
        {{ title }}
      </h1>
      <p
        v-if="description"
        class="text-[13.5px] leading-snug text-ink-soft max-w-2xl"
      >
        {{ description }}
      </p>
    </div>
    <div class="flex items-center gap-2 shrink-0">
      <div
        v-if="meta"
        class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data hidden md:block"
      >
        {{ meta }}
      </div>
      <slot name="actions" />
    </div>
  </div>
</template>
