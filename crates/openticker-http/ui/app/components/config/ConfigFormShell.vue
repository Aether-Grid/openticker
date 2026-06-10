<script setup lang="ts">
import type { ConfigViolation } from '~/types/api'

/**
 * Editor chrome shared by every config editor page. Wraps the form body with the
 * standard error/warning surfaces (stale-on-disk, server conflict/violations,
 * general error) and a sticky save bar. The page owns the form engine
 * (useConfigForm) and feeds its reactive flags in; this component is purely
 * presentational and emits intents back up.
 *
 * Render order:
 *   1. yellow "stale" alert (config changed on disk) with Reload / Keep editing
 *   2. red conflict / unmapped-violations alert listing each message; a default
 *      `#conflict-actions` slot lets a page add an action (e.g. "Stop bot and retry")
 *   3. a general-error banner when `generalError` is set
 *   4. the default slot (the form body)
 *   5. a sticky bottom save bar (dirty indicator + Discard / Save)
 */
const props = withDefaults(
  defineProps<{
    dirty: boolean
    saving: boolean
    stale: boolean
    conflict: boolean
    generalError?: string | null
    unmappedViolations?: ConfigViolation[]
    pending?: boolean
  }>(),
  {
    generalError: null,
    unmappedViolations: () => [],
    pending: false
  }
)

const emit = defineEmits<{
  save: []
  discard: []
  reload: []
}>()

const hasConflict = computed(() => props.conflict || (props.unmappedViolations?.length ?? 0) > 0)
</script>

<template>
  <div class="relative pb-24">
    <div class="space-y-4">
      <UAlert
        v-if="stale"
        color="warning"
        variant="subtle"
        icon="i-lucide-triangle-alert"
        title="Config changed on disk while you were editing"
        description="Saving may be rejected. Reload to pick up the latest, or keep editing and try anyway."
        :ui="{ root: 'rounded-lg' }"
      >
        <template #actions>
          <UButton
            color="warning"
            variant="solid"
            size="xs"
            label="Reload latest"
            @click="emit('reload')"
          />
          <UButton
            color="neutral"
            variant="ghost"
            size="xs"
            label="Keep editing"
          />
        </template>
      </UAlert>

      <UAlert
        v-if="hasConflict"
        color="error"
        variant="subtle"
        icon="i-lucide-octagon-alert"
        title="This change was rejected"
        :ui="{ root: 'rounded-lg' }"
      >
        <template #description>
          <ul
            v-if="unmappedViolations && unmappedViolations.length"
            class="space-y-1 mt-1"
          >
            <li
              v-for="(violation, idx) in unmappedViolations"
              :key="idx"
              class="text-[12.5px]"
            >
              <span class="font-data text-[11px] uppercase tracking-[0.08em] opacity-70"
                >{{ violation.scope }} ·
              </span>
              {{ violation.message }}
            </li>
          </ul>
          <span v-else>The runtime refused this change. Resolve the conflict and try again.</span>
          <div class="mt-3">
            <slot name="conflict-actions" />
          </div>
        </template>
      </UAlert>

      <UAlert
        v-if="generalError"
        color="error"
        variant="subtle"
        icon="i-lucide-triangle-alert"
        :title="generalError"
        :ui="{ root: 'rounded-lg' }"
      />

      <div
        v-if="pending"
        class="surface-card p-6"
      >
        <LoadingSkeleton />
      </div>
      <slot v-else />
    </div>

    <div
      class="sticky bottom-0 left-0 right-0 mt-8 -mx-8 px-8 py-3 surface-card border-t border-[color:var(--color-hairline)] flex items-center gap-3 rounded-none"
    >
      <div
        class="text-[11px] uppercase tracking-[0.14em] font-data transition-colors"
        :class="dirty ? 'text-[color:var(--color-pastel-yellow-ink)]' : 'text-ink-soft'"
      >
        <span
          v-if="dirty"
          class="inline-flex items-center gap-1.5"
        >
          <span class="w-1.5 h-1.5 rounded-full bg-[color:var(--color-pastel-yellow-ink)]" />
          Unsaved changes
        </span>
        <span v-else>Up to date</span>
      </div>
      <div class="ml-auto flex items-center gap-2">
        <UButton
          color="neutral"
          variant="ghost"
          size="sm"
          label="Discard"
          :disabled="!dirty"
          @click="emit('discard')"
        />
        <UButton
          color="neutral"
          variant="solid"
          size="sm"
          icon="i-lucide-save"
          label="Save"
          :loading="saving"
          :disabled="!dirty"
          @click="emit('save')"
        />
      </div>
    </div>
  </div>
</template>
