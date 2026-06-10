<script setup lang="ts">
/**
 * v-model-compatible chip editor for string arrays (e.g. bot `symbols`,
 * account `cash_balance_assets`). Wraps UInputTags. When `uppercase` is set each
 * entry is trimmed and upper-cased before it is emitted, which matches how the
 * backend normalizes tickers. Empty/whitespace-only entries are dropped.
 */
const props = withDefaults(
  defineProps<{
    modelValue: string[]
    uppercase?: boolean
    placeholder?: string
  }>(),
  {
    uppercase: false,
    placeholder: 'Type and press Enter…'
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: string[]]
}>()

function normalize(value: string[]): string[] {
  const out: string[] = []
  for (const raw of value) {
    const trimmed = raw.trim()
    if (!trimmed) continue
    out.push(props.uppercase ? trimmed.toUpperCase() : trimmed)
  }
  return out
}

const tags = computed<string[]>({
  get: () => props.modelValue ?? [],
  set: (value) => emit('update:modelValue', normalize(value))
})
</script>

<template>
  <UInputTags
    v-model="tags"
    :placeholder="placeholder"
    class="font-data w-full"
  />
</template>
