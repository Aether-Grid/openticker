<script setup lang="ts">
import type { RiskOverrides, RiskProfileConfig } from '~/types/api'

/**
 * Renders the eight numeric risk fields shared by a RiskProfileConfig (the risk
 * profile page) and a RiskOverrides map (per-bot risk overrides). The two modes
 * differ only in optionality:
 *
 *  - `nullable: false` — every field is a required number (profile page).
 *  - `nullable: true`  — every field is optional. A null/undefined value means
 *    "inherit from the assigned profile"; the input shows the inherited value as
 *    a placeholder (when `inherited` is supplied) and a clear button lets the
 *    user fall back to inheritance.
 *
 * Error lookups are delegated to `errors(path)`; `pathPrefix` lets the bot page
 * resolve `risk.overrides.max_spread_bps` while the profile page uses the bare
 * `max_spread_bps`.
 */

type RiskFieldKey =
  | 'max_daily_loss_pct'
  | 'max_open_positions'
  | 'target_order_notional_usd'
  | 'max_order_notional_usd'
  | 'max_spread_bps'
  | 'max_slippage_bps'
  | 'stale_data_ms'
  | 'cooldown_after_reject_ms'

type RiskModel = RiskProfileConfig | RiskOverrides

const props = withDefaults(
  defineProps<{
    modelValue: RiskModel
    nullable: boolean
    errors?: (path: string) => string | undefined
    pathPrefix?: string
    /** Inherited profile values shown as placeholders when a field is cleared. */
    inherited?: Partial<RiskProfileConfig>
  }>(),
  {
    errors: undefined,
    pathPrefix: '',
    inherited: undefined
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: RiskModel]
}>()

interface FieldSpec {
  key: RiskFieldKey
  label: string
  hint?: string
  step?: number
  min?: number
}

const FIELDS: FieldSpec[] = [
  { key: 'max_daily_loss_pct', label: 'Max daily loss %', hint: '0–100', step: 0.1, min: 0 },
  { key: 'max_open_positions', label: 'Max open positions', step: 1, min: 0 },
  { key: 'target_order_notional_usd', label: 'Target order notional USD', step: 1, min: 0 },
  { key: 'max_order_notional_usd', label: 'Max order notional USD', step: 1, min: 0 },
  { key: 'max_spread_bps', label: 'Max spread bps', step: 1, min: 0 },
  { key: 'max_slippage_bps', label: 'Max slippage bps', step: 1, min: 0 },
  { key: 'stale_data_ms', label: 'Stale data window ms', step: 1, min: 0 },
  { key: 'cooldown_after_reject_ms', label: 'Cooldown after reject ms', step: 1, min: 0 }
]

function pathFor(key: RiskFieldKey): string {
  return props.pathPrefix ? `${props.pathPrefix}.${key}` : key
}

function errorFor(key: RiskFieldKey): string | undefined {
  return props.errors ? props.errors(pathFor(key)) : undefined
}

function valueOf(key: RiskFieldKey): number | null {
  const raw = (props.modelValue as Record<string, unknown>)[key]
  return typeof raw === 'number' ? raw : null
}

function inheritedOf(key: RiskFieldKey): number | null {
  const raw = props.inherited?.[key as keyof RiskProfileConfig]
  return typeof raw === 'number' ? raw : null
}

function placeholderFor(key: RiskFieldKey): string {
  const inh = inheritedOf(key)
  if (props.nullable && inh !== null) return `Inherit (${inh})`
  if (props.nullable) return 'Inherit'
  return ''
}

function setValue(key: RiskFieldKey, value: number | null) {
  emit('update:modelValue', { ...props.modelValue, [key]: value })
}

/** Clears a field back to inheritance (overrides only). */
function clearValue(key: RiskFieldKey) {
  emit('update:modelValue', { ...props.modelValue, [key]: null })
}
</script>

<template>
  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
    <ConfigField
      v-for="field in FIELDS"
      :key="field.key"
      :label="field.label"
      :hint="field.hint"
      :name="pathFor(field.key)"
      :error="errorFor(field.key)"
    >
      <div class="flex items-center gap-1.5">
        <UInputNumber
          :model-value="valueOf(field.key) ?? undefined"
          :step="field.step"
          :min="field.min"
          :placeholder="placeholderFor(field.key)"
          class="font-data w-full"
          @update:model-value="(v: number | undefined) => setValue(field.key, v ?? null)"
        />
        <UTooltip
          v-if="nullable && valueOf(field.key) !== null"
          text="Clear and inherit from profile"
        >
          <UButton
            color="neutral"
            variant="ghost"
            size="xs"
            icon="i-lucide-undo-2"
            aria-label="Inherit from profile"
            @click="clearValue(field.key)"
          />
        </UTooltip>
      </div>
    </ConfigField>
  </div>
</template>
