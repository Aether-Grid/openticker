<script setup lang="ts">
import type { IndicatorInstanceConfig, IndicatorRole, IndicatorSignalPolicy } from '~/types/api'

/**
 * Editor for a bot's `indicators: IndicatorInstanceConfig[]`.
 *
 * v-model is the full array. Following the immutable-emit contract used by
 * ConfigRiskFields / ConfigWatchlistEditor, this component NEVER mutates the
 * bound array or its rows: every edit emits a freshly cloned array of cloned
 * rows so the owning useConfigForm draft tracks dirtiness correctly. The nested
 * `params` map is delegated to ConfigParamsTable, which also emits immutably.
 *
 * `errors(path)` resolves inline validation for dotted paths like
 * `indicators[0].id` / `indicators[0].type`, matching `validateBot`. When
 * `disabled` (the bot is running and the indicator set is locked) every control
 * is rendered inside a disabled fieldset so it is non-interactive.
 */
const props = withDefaults(
  defineProps<{
    modelValue: IndicatorInstanceConfig[]
    errors?: (path: string) => string | undefined
    disabled?: boolean
  }>(),
  {
    errors: undefined,
    disabled: false
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: IndicatorInstanceConfig[]]
}>()

const ROLE_ITEMS: { label: string; value: IndicatorRole }[] = [
  { label: 'primary signal', value: 'primary_signal' },
  { label: 'filter', value: 'filter' },
  { label: 'context', value: 'context' },
  { label: 'risk helper', value: 'risk_helper' },
  { label: 'research only', value: 'research_only' }
]

const SIGNAL_POLICY_ITEMS: { label: string; value: IndicatorSignalPolicy }[] = [
  { label: 'preview allowed', value: 'preview_allowed' },
  { label: 'confirmed required', value: 'confirmed_required' }
]

/**
 * Synthetic per-row keys for the row `v-for`. Component-local only, never
 * emitted into the saved array. Mirror modelValue by position: patchRow keeps
 * length/order so keys stay stable; add/remove/reorder and external length
 * changes reconcile below.
 */
let nextRowKey = 0
function freshKey(): number {
  return nextRowKey++
}
const rowKeys = ref<number[]>(props.modelValue.map(() => freshKey()))

watch(
  () => props.modelValue.length,
  (len) => {
    if (rowKeys.value.length === len) return
    if (len < rowKeys.value.length) {
      rowKeys.value = rowKeys.value.slice(0, len)
    } else {
      while (rowKeys.value.length < len) rowKeys.value.push(freshKey())
    }
  }
)

// Free-text type suggestions gathered from the types already present across the
// list, so a user adding a second RSI indicator can pick the same key. Free
// text is still allowed (USelectMenu `create-item`).
const typeSuggestions = computed(() => {
  const seen = new Set<string>()
  for (const ind of props.modelValue) {
    const t = ind.type?.trim()
    if (t) seen.add(t)
  }
  return Array.from(seen).sort().map((t) => ({ label: t, value: t }))
})

function pathFor(index: number, field: string): string {
  return `indicators[${index}].${field}`
}

function errorFor(index: number, field: string): string | undefined {
  return props.errors ? props.errors(pathFor(index, field)) : undefined
}

/** Clones a row (shallow + a fresh params object) for the immutable emit. */
function cloneRow(row: IndicatorInstanceConfig): IndicatorInstanceConfig {
  return { ...row, params: { ...row.params } }
}

/** Emits a new array with the row at `index` replaced by `{ ...row, ...patch }`. */
function patchRow(index: number, patch: Partial<IndicatorInstanceConfig>) {
  const next = props.modelValue.map((row, i) => (i === index ? { ...cloneRow(row), ...patch } : cloneRow(row)))
  emit('update:modelValue', next)
}

function setId(index: number, value: string) {
  patchRow(index, { id: value })
}

function setType(index: number, value: string) {
  patchRow(index, { type: value ?? '' })
}

function setEnabled(index: number, value: boolean) {
  patchRow(index, { enabled: value })
}

// role / signal_policy are nullable selects: an empty/cleared selection maps to
// undefined ("inherit the strategy default" on the backend).
function setRole(index: number, value: IndicatorRole | undefined) {
  patchRow(index, { role: value ?? null })
}

function setSignalPolicy(index: number, value: IndicatorSignalPolicy | undefined) {
  patchRow(index, { signal_policy: value ?? null })
}

function setWeight(index: number, value: number | undefined) {
  patchRow(index, { weight: value === undefined ? null : value })
}

function setParams(index: number, value: Record<string, unknown>) {
  patchRow(index, { params: value })
}

function addIndicator() {
  const next: IndicatorInstanceConfig = {
    id: '',
    type: '',
    enabled: true,
    params: {}
  }
  rowKeys.value.push(freshKey())
  emit('update:modelValue', [...props.modelValue.map(cloneRow), next])
}

function removeIndicator(index: number) {
  rowKeys.value.splice(index, 1)
  emit('update:modelValue', props.modelValue.filter((_, i) => i !== index).map(cloneRow))
}

/** Swaps the rows at `index` and `index + dir`, keeping the synthetic keys aligned. */
function move(index: number, dir: -1 | 1) {
  const target = index + dir
  if (target < 0 || target >= props.modelValue.length) return
  const next = props.modelValue.map(cloneRow)
  const tmp = next[index]!
  next[index] = next[target]!
  next[target] = tmp
  const keys = rowKeys.value.slice()
  const tmpKey = keys[index]!
  keys[index] = keys[target]!
  keys[target] = tmpKey
  rowKeys.value = keys
  emit('update:modelValue', next)
}
</script>

<template>
  <fieldset
    :disabled="disabled"
    class="border-0 p-0 m-0 min-w-0 space-y-3"
    :class="disabled ? 'opacity-60' : ''"
  >
    <div
      v-if="modelValue.length === 0"
      class="surface-card p-5 flex items-center justify-between gap-4"
    >
      <span class="text-[13px] text-ink-soft">No indicators</span>
      <UButton
        color="neutral"
        variant="outline"
        size="sm"
        icon="i-lucide-plus"
        label="Add indicator"
        :disabled="disabled"
        @click="addIndicator"
      />
    </div>

    <template v-else>
      <div
        v-for="(indicator, index) in modelValue"
        :key="rowKeys[index] ?? index"
        class="surface-card p-4 space-y-4 border border-[color:var(--color-hairline)]"
      >
        <!-- Header: identity + reorder/remove -->
        <div class="flex items-start gap-3">
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 flex-1 min-w-0">
            <ConfigField
              label="Indicator id"
              :error="errorFor(index, 'id')"
            >
              <UInput
                :model-value="indicator.id"
                placeholder="e.g. fast_ema"
                class="font-data w-full"
                @update:model-value="(v: string) => setId(index, v)"
              />
            </ConfigField>
            <ConfigField
              label="Type"
              :error="errorFor(index, 'type')"
            >
              <USelectMenu
                :model-value="indicator.type"
                :items="typeSuggestions"
                value-key="value"
                create-item
                placeholder="e.g. sma_crossover"
                class="font-data w-full"
                @update:model-value="(v: string) => setType(index, v)"
                @create="(v: string) => setType(index, v)"
              />
            </ConfigField>
          </div>
          <div class="flex items-center gap-0.5 pt-5">
            <UTooltip text="Move up">
              <UButton
                color="neutral"
                variant="ghost"
                size="xs"
                icon="i-lucide-chevron-up"
                aria-label="Move up"
                :disabled="disabled || index === 0"
                @click="move(index, -1)"
              />
            </UTooltip>
            <UTooltip text="Move down">
              <UButton
                color="neutral"
                variant="ghost"
                size="xs"
                icon="i-lucide-chevron-down"
                aria-label="Move down"
                :disabled="disabled || index === modelValue.length - 1"
                @click="move(index, 1)"
              />
            </UTooltip>
            <UTooltip text="Remove indicator">
              <UButton
                color="error"
                variant="ghost"
                size="xs"
                icon="i-lucide-trash-2"
                aria-label="Remove indicator"
                :disabled="disabled"
                @click="removeIndicator(index)"
              />
            </UTooltip>
          </div>
        </div>

        <!-- Behaviour -->
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          <ConfigField label="Enabled">
            <USwitch
              :model-value="indicator.enabled"
              @update:model-value="(v: boolean) => setEnabled(index, v)"
            />
          </ConfigField>
          <ConfigField
            label="Role"
            hint="Empty = strategy default"
          >
            <USelect
              :model-value="indicator.role ?? undefined"
              :items="ROLE_ITEMS"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: IndicatorRole | undefined) => setRole(index, v)"
            />
          </ConfigField>
          <ConfigField
            label="Signal policy"
            hint="Empty = strategy default"
          >
            <USelect
              :model-value="indicator.signal_policy ?? undefined"
              :items="SIGNAL_POLICY_ITEMS"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: IndicatorSignalPolicy | undefined) => setSignalPolicy(index, v)"
            />
          </ConfigField>
          <ConfigField
            label="Weight"
            hint="Optional"
            :error="errorFor(index, 'weight')"
          >
            <UInputNumber
              :model-value="indicator.weight ?? undefined"
              :step="0.1"
              placeholder="—"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setWeight(index, v)"
            />
          </ConfigField>
        </div>

        <!-- Params -->
        <div class="space-y-2 pt-1">
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Parameters</span>
          <ConfigParamsTable
            :model-value="indicator.params"
            :path-prefix="`indicators[${index}].params`"
            :errors="errors"
            @update:model-value="(v: Record<string, unknown>) => setParams(index, v)"
          />
        </div>
      </div>

      <div class="flex justify-end">
        <UButton
          color="neutral"
          variant="outline"
          size="sm"
          icon="i-lucide-plus"
          label="Add indicator"
          :disabled="disabled"
          @click="addIndicator"
        />
      </div>
    </template>
  </fieldset>
</template>
