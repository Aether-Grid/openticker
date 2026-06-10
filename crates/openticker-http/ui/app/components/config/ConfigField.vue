<script setup lang="ts">
/**
 * Thin wrapper over UFormField that standardizes the house label style and adds
 * an optional "locked" affordance (a lock icon + tooltip explaining why the
 * field cannot currently be edited). Pages pass `:error="form.errorFor('path')"`
 * so a server/client validation message renders under the input.
 *
 * The actual input lives in the default slot. When `locked`, the slot content is
 * rendered inside a `disabled` fieldset so any control inside is non-interactive
 * without each editor page having to thread a `disabled` prop through.
 */
defineProps<{
  label: string
  error?: string
  hint?: string
  name?: string
  locked?: boolean
  lockReason?: string
}>()
</script>

<template>
  <UFormField
    :label="label"
    :error="error"
    :name="name"
    :ui="{ label: 'text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data' }"
  >
    <template
      v-if="hint || locked"
      #hint
    >
      <span class="flex items-center gap-1.5">
        <span
          v-if="hint"
          class="text-[11px] text-ink-soft font-data normal-case tracking-normal"
        >
          {{ hint }}
        </span>
        <UTooltip
          v-if="locked"
          :text="lockReason ?? 'This field is locked.'"
        >
          <UIcon
            name="i-lucide-lock"
            class="size-3.5 text-ink-soft"
          />
        </UTooltip>
      </span>
    </template>

    <fieldset
      :disabled="locked"
      class="border-0 p-0 m-0 min-w-0"
      :class="locked ? 'opacity-60' : ''"
    >
      <slot />
    </fieldset>
  </UFormField>
</template>
