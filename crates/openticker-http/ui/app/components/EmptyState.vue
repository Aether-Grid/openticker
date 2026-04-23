<script setup lang="ts">
defineProps<{
  icon?: string
  title?: string
  description?: string
}>()
</script>

<template>
  <div class="surface-card p-10 text-center flex flex-col items-center gap-3">
    <div class="w-10 h-10 rounded-full bg-[color:var(--color-canvas)] grid place-items-center hairline">
      <UIcon
        :name="icon ?? 'i-lucide-inbox'"
        class="size-5 text-ink-soft"
      />
    </div>
    <div class="font-editorial text-[22px] text-ink">
      {{ title ?? 'Nothing here yet' }}
    </div>
    <p
      v-if="description"
      class="text-[13px] text-ink-soft max-w-md"
    >
      {{ description }}
    </p>
    <slot />
  </div>
</template>
