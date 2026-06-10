<script setup lang="ts">
import type { DataPlaneWatchlistEntry, Timeframe } from '~/types/api'

/**
 * Editor for the data-plane watchlist (`GlobalConfig.data_plane.watchlist`).
 *
 * v-model is the full `DataPlaneWatchlistEntry[]`. Following the immutable-emit
 * contract used by ConfigRiskFields, this component NEVER mutates the bound
 * array or its rows: every edit emits a freshly cloned array of cloned rows, so
 * the owning useConfigForm draft tracks dirtiness correctly.
 *
 * `accounts` supplies the account-id options for each row's account select.
 * `errors(path)` resolves inline validation messages for dotted paths like
 * `data_plane.watchlist[0].symbol`, matching `validateGlobal`.
 */
const props = withDefaults(
  defineProps<{
    modelValue: DataPlaneWatchlistEntry[]
    accounts: string[]
    errors?: (path: string) => string | undefined
  }>(),
  {
    errors: undefined
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: DataPlaneWatchlistEntry[]]
}>()

const TIMEFRAMES: Timeframe[] = ['1m', '5m', '15m', '30m', '1h', '4h', '1d']

const accountItems = computed(() => props.accounts.map((id) => ({ label: id, value: id })))
const timeframeItems = TIMEFRAMES.map((tf) => ({ label: tf, value: tf }))

function pathFor(index: number, key: keyof DataPlaneWatchlistEntry): string {
  return `data_plane.watchlist[${index}].${key}`
}

function errorFor(index: number, key: keyof DataPlaneWatchlistEntry): string | undefined {
  return props.errors ? props.errors(pathFor(index, key)) : undefined
}

/** Emits a new array with the row at `index` replaced by `{ ...row, ...patch }`. */
function patchRow(index: number, patch: Partial<DataPlaneWatchlistEntry>) {
  const next = props.modelValue.map((row, i) => (i === index ? { ...row, ...patch } : { ...row }))
  emit('update:modelValue', next)
}

function setAccount(index: number, value: string) {
  patchRow(index, { account: value })
}

function setSymbol(index: number, value: string) {
  patchRow(index, { symbol: value.toUpperCase().trim() })
}

function setTimeframe(index: number, value: Timeframe) {
  patchRow(index, { timeframe: value })
}

function setPollingInterval(index: number, value: number | undefined) {
  patchRow(index, { polling_interval_ms: value === undefined ? null : value })
}

function setRetention(index: number, value: number | undefined) {
  patchRow(index, { retention: value === undefined ? null : value })
}

function removeRow(index: number) {
  emit(
    'update:modelValue',
    props.modelValue.filter((_, i) => i !== index).map((row) => ({ ...row }))
  )
}

function addRow() {
  const next: DataPlaneWatchlistEntry = {
    account: props.accounts[0] ?? '',
    symbol: '',
    timeframe: '5m',
    polling_interval_ms: null,
    retention: null
  }
  emit('update:modelValue', [...props.modelValue.map((row) => ({ ...row })), next])
}
</script>

<template>
  <div class="space-y-3">
    <div
      v-if="modelValue.length === 0"
      class="surface-card p-5 flex items-center justify-between gap-4"
    >
      <span class="text-[13px] text-ink-soft">No watchlist entries</span>
      <UButton
        color="neutral"
        variant="outline"
        size="sm"
        icon="i-lucide-plus"
        label="Add entry"
        @click="addRow"
      />
    </div>

    <template v-else>
      <div class="surface-card overflow-hidden">
        <!-- Column headers -->
        <div
          class="hidden lg:grid grid-cols-[minmax(0,1.2fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_minmax(0,1fr)_minmax(0,1fr)_auto] gap-3 px-4 py-2.5 border-b border-[color:var(--color-hairline)]"
        >
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Account</span>
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Symbol</span>
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Timeframe</span>
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Polling ms</span>
          <span class="text-[10.5px] uppercase tracking-[0.18em] text-ink-soft font-data">Retention</span>
          <span class="w-8" />
        </div>

        <!-- Rows -->
        <div
          v-for="(entry, index) in modelValue"
          :key="index"
          class="grid grid-cols-1 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_minmax(0,1fr)_minmax(0,1fr)_auto] gap-3 px-4 py-3 border-b border-[color:var(--color-hairline)] last:border-b-0"
        >
          <ConfigField
            label="Account"
            class="lg:hidden"
            :error="errorFor(index, 'account')"
          >
            <USelect
              :model-value="entry.account"
              :items="accountItems"
              placeholder="Select account"
              class="font-data w-full"
              @update:model-value="(v: string) => setAccount(index, v)"
            />
          </ConfigField>
          <div class="hidden lg:block">
            <USelect
              :model-value="entry.account"
              :items="accountItems"
              placeholder="Select account"
              class="font-data w-full"
              @update:model-value="(v: string) => setAccount(index, v)"
            />
            <p
              v-if="errorFor(index, 'account')"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-red-ink)]"
            >
              {{ errorFor(index, 'account') }}
            </p>
          </div>

          <ConfigField
            label="Symbol"
            class="lg:hidden"
            :error="errorFor(index, 'symbol')"
          >
            <UInput
              :model-value="entry.symbol"
              placeholder="BTC-USDT"
              class="font-data w-full"
              @update:model-value="(v: string) => setSymbol(index, v)"
            />
          </ConfigField>
          <div class="hidden lg:block">
            <UInput
              :model-value="entry.symbol"
              placeholder="BTC-USDT"
              class="font-data w-full"
              @update:model-value="(v: string) => setSymbol(index, v)"
            />
            <p
              v-if="errorFor(index, 'symbol')"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-red-ink)]"
            >
              {{ errorFor(index, 'symbol') }}
            </p>
          </div>

          <ConfigField
            label="Timeframe"
            class="lg:hidden"
            :error="errorFor(index, 'timeframe')"
          >
            <USelect
              :model-value="entry.timeframe"
              :items="timeframeItems"
              class="font-data w-full"
              @update:model-value="(v: Timeframe) => setTimeframe(index, v)"
            />
          </ConfigField>
          <div class="hidden lg:block">
            <USelect
              :model-value="entry.timeframe"
              :items="timeframeItems"
              class="font-data w-full"
              @update:model-value="(v: Timeframe) => setTimeframe(index, v)"
            />
          </div>

          <ConfigField
            label="Polling ms"
            hint="Empty = inherit default"
            class="lg:hidden"
            :error="errorFor(index, 'polling_interval_ms')"
          >
            <UInputNumber
              :model-value="entry.polling_interval_ms ?? undefined"
              :min="250"
              :step="250"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setPollingInterval(index, v)"
            />
          </ConfigField>
          <div class="hidden lg:block">
            <UInputNumber
              :model-value="entry.polling_interval_ms ?? undefined"
              :min="250"
              :step="250"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setPollingInterval(index, v)"
            />
            <p
              v-if="errorFor(index, 'polling_interval_ms')"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-red-ink)]"
            >
              {{ errorFor(index, 'polling_interval_ms') }}
            </p>
          </div>

          <ConfigField
            label="Retention"
            hint="Empty = inherit default"
            class="lg:hidden"
            :error="errorFor(index, 'retention')"
          >
            <UInputNumber
              :model-value="entry.retention ?? undefined"
              :min="1"
              :step="1"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setRetention(index, v)"
            />
          </ConfigField>
          <div class="hidden lg:block">
            <UInputNumber
              :model-value="entry.retention ?? undefined"
              :min="1"
              :step="1"
              placeholder="Default"
              class="font-data w-full"
              @update:model-value="(v: number | undefined) => setRetention(index, v)"
            />
            <p
              v-if="errorFor(index, 'retention')"
              class="mt-1 text-[11px] text-[color:var(--color-pastel-red-ink)]"
            >
              {{ errorFor(index, 'retention') }}
            </p>
          </div>

          <div class="flex items-start justify-end lg:items-center">
            <UTooltip text="Remove entry">
              <UButton
                color="neutral"
                variant="ghost"
                size="xs"
                icon="i-lucide-trash-2"
                aria-label="Remove entry"
                @click="removeRow(index)"
              />
            </UTooltip>
          </div>
        </div>
      </div>

      <div class="flex justify-end">
        <UButton
          color="neutral"
          variant="outline"
          size="sm"
          icon="i-lucide-plus"
          label="Add entry"
          @click="addRow"
        />
      </div>
    </template>
  </div>
</template>
