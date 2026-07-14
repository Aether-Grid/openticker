<script setup lang="ts">
import { deepEqual } from '~/utils/deepEqual'

/**
 * Schema-less key/typed-value editor for an open `Record<string, unknown>` map
 * (an indicator's `params` table on the backend). v-model is the whole object.
 *
 * Following the immutable-emit contract used by ConfigRiskFields /
 * ConfigWatchlistEditor, this component NEVER mutates the bound object: every
 * edit emits a freshly built object so the owning useConfigForm draft tracks
 * dirtiness correctly.
 *
 * Each row carries a stable synthetic key (component-local only, never emitted)
 * so renaming a key, switching its value type, or removing a middle row does not
 * strand transient widget state on the wrong row. The rows are the source of
 * truth while editing; the emitted object is rebuilt from the rows on every
 * change. Duplicate / empty keys are surfaced as a per-row hint and are dropped
 * from the emitted object rather than crashing.
 */
// Scalar rows are freely editable. A `structured` row holds a TOML table/array
// (object/array value) and is PRESERVED read-only: we keep the original value
// reference verbatim and never let the user coerce it into a string, so a loaded
// structured param round-trips to the server unchanged when sibling scalar
// params are edited.
type ScalarType = 'string' | 'number' | 'boolean'
type ValueType = ScalarType | 'structured'

interface ParamRow {
  /** Synthetic stable key for the v-for; never emitted into params. */
  rowKey: number
  key: string
  type: ValueType
  /**
   * Scalar rows narrow to string | number | boolean. Structured rows hold the
   * original object/array value unchanged (preserved, not stringified-then-
   * reparsed) plus a read-only JSON rendering for display.
   */
  value: unknown
  /** JSON preview shown for structured rows; ignored for scalar rows. */
  display?: string
}

const props = withDefaults(
  defineProps<{
    modelValue: Record<string, unknown>
    errors?: (path: string) => string | undefined
    pathPrefix?: string
  }>(),
  {
    errors: undefined,
    pathPrefix: ''
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: Record<string, unknown>]
}>()

let nextRowKey = 0
function freshKey(): number {
  return nextRowKey++
}

/** True for object/array (non-null) values: a TOML table/array we preserve. */
function isStructured(value: unknown): boolean {
  return typeof value === 'object' && value !== null
}

/** Infers the editor value-type from a raw param value. */
function typeOf(value: unknown): ValueType {
  if (typeof value === 'number') return 'number'
  if (typeof value === 'boolean') return 'boolean'
  if (isStructured(value)) return 'structured'
  return 'string'
}

/** Coerces a raw param value into the row's displayable value for a scalar type. */
function coerce(value: unknown, type: ScalarType): string | number | boolean {
  if (type === 'number') return typeof value === 'number' ? value : 0
  if (type === 'boolean') return typeof value === 'boolean' ? value : false
  // Strings keep their text; non-string scalars are stringified.
  if (typeof value === 'string') return value
  if (value === null || value === undefined) return ''
  return String(value)
}

function rowsFromModel(model: Record<string, unknown>): ParamRow[] {
  return Object.entries(model).map(([key, value]) => {
    const type = typeOf(value)
    if (type === 'structured') {
      // Keep the original value reference verbatim; only the display is derived.
      return { rowKey: freshKey(), key, type, value, display: JSON.stringify(value) }
    }
    return { rowKey: freshKey(), key, type, value: coerce(value, type) }
  })
}

// The rows are the editing source of truth. They are seeded from modelValue and
// re-seeded only when the parent replaces the object out of band (reload /
// discard) — detected by structural difference against the emitted projection.
const rows = ref<ParamRow[]>(rowsFromModel(props.modelValue))

/** Projects the rows back into a params object, dropping empty/duplicate keys. */
function project(rs: ParamRow[]): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const row of rs) {
    const key = row.key.trim()
    if (!key) continue
    if (key in out) continue // first writer wins; duplicates are flagged in the UI
    out[key] = row.value
  }
  return out
}

function emitRows() {
  emit('update:modelValue', project(rows.value))
}

// Re-seed rows when the parent object changes in a way the current rows do not
// already represent (reload / discard / external replace). Guarding on a deep
// comparison against our own projection prevents the echo of our own emit from
// clobbering in-progress edits (e.g. a half-typed key).
watch(
  () => props.modelValue,
  (model) => {
    if (deepEqual(model, project(rows.value))) return
    rows.value = rowsFromModel(model)
  },
  { deep: true }
)

// The type selector only offers scalar types; structured rows are read-only and
// render a disabled selector (see template) so they cannot be coerced.
const ROW_TYPE_ITEMS = [
  { label: 'text', value: 'string' as ScalarType },
  { label: 'number', value: 'number' as ScalarType },
  { label: 'boolean', value: 'boolean' as ScalarType }
]

/** Keys that are non-empty and appear more than once across the rows. */
const duplicateKeys = computed(() => {
  const counts = new Map<string, number>()
  for (const row of rows.value) {
    const key = row.key.trim()
    if (!key) continue
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  return new Set(Array.from(counts.entries()).filter(([, n]) => n > 1).map(([k]) => k))
})

/** Per-row hint shown when a key is empty or duplicated (not a hard error). */
function rowHint(index: number): string | undefined {
  const row = rows.value[index]
  if (!row) return undefined
  const key = row.key.trim()
  if (!key) return 'Key is empty — this parameter will be dropped'
  if (duplicateKeys.value.has(key)) return 'Duplicate key — only the first is kept'
  return undefined
}

/** Server/client validation message for a param path, when one is bound. */
function errorFor(index: number): string | undefined {
  if (!props.errors) return undefined
  const key = rows.value[index]?.key.trim()
  if (!key) return undefined
  const path = props.pathPrefix ? `${props.pathPrefix}.${key}` : key
  return props.errors(path)
}

function setKey(index: number, value: string) {
  const row = rows.value[index]
  if (!row) return
  row.key = value
  emitRows()
}

function setStringValue(index: number, value: string) {
  const row = rows.value[index]
  if (!row || row.type === 'structured') return
  row.value = value
  emitRows()
}

function setNumberValue(index: number, value: number | undefined) {
  const row = rows.value[index]
  if (!row || row.type === 'structured') return
  row.value = value ?? 0
  emitRows()
}

function setBooleanValue(index: number, value: boolean) {
  const row = rows.value[index]
  if (!row || row.type === 'structured') return
  row.value = value
  emitRows()
}

/**
 * Switches a scalar row's value type, coercing the current value into the new
 * type. Structured rows are read-only, so conversions to/from `structured` are
 * rejected (the selector is disabled for them anyway).
 */
function setType(index: number, type: ScalarType) {
  const row = rows.value[index]
  if (!row || row.type === 'structured' || row.type === type) return
  row.value = coerce(row.value, type)
  row.type = type
  emitRows()
}

function addRow() {
  rows.value.push({ rowKey: freshKey(), key: '', type: 'string', value: '' })
  // Do not emit yet: an empty key projects to nothing, so emitting would be a
  // no-op while leaving the new (empty) row visible for the user to fill in.
}

function removeRow(index: number) {
  rows.value.splice(index, 1)
  emitRows()
}
</script>

<template>
  <div class="space-y-2.5">
    <div
      v-if="rows.length === 0"
      class="flex items-center justify-between gap-4 rounded-md border border-[color:var(--color-hairline)] px-3.5 py-2.5"
    >
      <span class="text-[12.5px] text-ink-soft">No parameters</span>
      <UButton
        color="neutral"
        variant="ghost"
        size="xs"
        icon="i-lucide-plus"
        label="Add parameter"
        @click="addRow"
      />
    </div>

    <template v-else>
      <div class="space-y-2">
        <div
          v-for="(row, index) in rows"
          :key="row.rowKey"
          class="grid grid-cols-1 sm:grid-cols-[minmax(0,1fr)_120px_minmax(0,1.4fr)_auto] gap-2 items-start"
        >
          <div>
            <UInput
              :model-value="row.key"
              placeholder="key"
              class="font-data w-full"
              @update:model-value="(v: string) => setKey(index, v)"
            />
            <p
              v-if="rowHint(index)"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-yellow-ink)]"
            >
              {{ rowHint(index) }}
            </p>
            <p
              v-if="errorFor(index)"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-red-ink)]"
            >
              {{ errorFor(index) }}
            </p>
          </div>

          <USelect
            v-if="row.type === 'structured'"
            :model-value="'structured'"
            :items="[{ label: 'structured', value: 'structured' }]"
            disabled
            class="font-data w-full"
          />
          <USelect
            v-else
            :model-value="row.type"
            :items="ROW_TYPE_ITEMS"
            class="font-data w-full"
            @update:model-value="(v: ScalarType) => setType(index, v)"
          />

          <div class="flex items-center min-h-[32px]">
            <UInputNumber
              v-if="row.type === 'number'"
              :model-value="(row.value as number)"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setNumberValue(index, v)"
            />
            <USwitch
              v-else-if="row.type === 'boolean'"
              :model-value="(row.value as boolean)"
              @update:model-value="(v: boolean) => setBooleanValue(index, v)"
            />
            <!-- Structured (TOML table/array): read-only JSON; value preserved verbatim. -->
            <UInput
              v-else-if="row.type === 'structured'"
              :model-value="row.display ?? ''"
              readonly
              :title="'Structured value (read-only)'"
              placeholder="structured value (read-only)"
              class="font-data w-full"
            />
            <UInput
              v-else
              :model-value="(row.value as string)"
              placeholder="value"
              class="font-data w-full"
              @update:model-value="(v: string) => setStringValue(index, v)"
            />
          </div>

          <UTooltip text="Remove parameter">
            <UButton
              color="neutral"
              variant="ghost"
              size="xs"
              icon="i-lucide-trash-2"
              aria-label="Remove parameter"
              @click="removeRow(index)"
            />
          </UTooltip>
        </div>
      </div>

      <div class="flex justify-end">
        <UButton
          color="neutral"
          variant="ghost"
          size="xs"
          icon="i-lucide-plus"
          label="Add parameter"
          @click="addRow"
        />
      </div>
    </template>
  </div>
</template>
