<script setup lang="ts">
import type { IndicatorSignalKind } from '~/types/api'

const props = defineProps<{
  botId: string
  botLabel?: string
  symbols?: string[]
  defaultPrice?: number | null
  defaultTimestamp?: string | null
}>()

const open = defineModel<boolean>('open', { default: false })

const emit = defineEmits<{
  applied: []
}>()

const actions = useServiceActions()

type SignalOption = {
  value: IndicatorSignalKind
  label: string
  direction: 'long' | 'short' | 'none'
  tone: 'green' | 'red' | 'neutral'
  phase: 'preview' | 'confirmed' | 'none'
}

const SIGNAL_OPTIONS: SignalOption[] = [
  { value: 'buy_preview', label: 'Buy preview', direction: 'long', tone: 'green', phase: 'preview' },
  { value: 'buy_confirmed', label: 'Buy confirmed', direction: 'long', tone: 'green', phase: 'confirmed' },
  { value: 'sell_preview', label: 'Sell preview', direction: 'short', tone: 'red', phase: 'preview' },
  { value: 'sell_confirmed', label: 'Sell confirmed', direction: 'short', tone: 'red', phase: 'confirmed' },
  { value: 'none', label: 'Neutral', direction: 'none', tone: 'neutral', phase: 'none' }
]

function toLocalInput(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

const signal = ref<IndicatorSignalKind>('buy_confirmed')
const price = ref<number | null>(props.defaultPrice ?? null)
const timestamp = ref<string>(toLocalInput(props.defaultTimestamp ?? new Date().toISOString()))
const symbol = ref<string>(props.symbols && props.symbols.length === 1 ? (props.symbols[0] ?? '') : '')
const submitting = ref(false)

const symbolItems = computed(() => {
  const opts = (props.symbols ?? []).map((s) => ({ label: s, value: s }))
  return [{ label: 'Default (primary)', value: '' }, ...opts]
})

const FALLBACK_SIGNAL = SIGNAL_OPTIONS[0] as SignalOption

const active = computed<SignalOption>(() => SIGNAL_OPTIONS.find((o) => o.value === signal.value) ?? FALLBACK_SIGNAL)

const invalid = computed(() => {
  if (price.value === null || price.value === undefined) return 'Enter a price.'
  if (Number.isNaN(Number(price.value))) return 'Price must be a number.'
  if (Number(price.value) <= 0) return 'Price must be positive.'
  if (!timestamp.value) return 'Choose a timestamp.'
  return null
})

watch(open, (isOpen) => {
  if (!isOpen) return
  price.value = props.defaultPrice ?? price.value ?? null
  timestamp.value = toLocalInput(props.defaultTimestamp ?? new Date().toISOString())
  if (props.symbols && props.symbols.length === 1) symbol.value = props.symbols[0] ?? ''
})

function stampNow() {
  timestamp.value = toLocalInput(new Date().toISOString())
}

async function submit() {
  if (invalid.value) return
  submitting.value = true
  const iso = new Date(timestamp.value).toISOString()
  const ok = await actions.manualSignal(props.botId, {
    signal: signal.value,
    price: Number(price.value),
    timestamp: iso,
    symbol: symbol.value || undefined
  })
  submitting.value = false
  if (ok) {
    open.value = false
    emit('applied')
  }
}
</script>

<template>
  <UModal
    v-model:open="open"
    :ui="{
      content: 'rounded-xl max-w-[560px]',
      header: 'border-b border-[color:var(--color-hairline)] px-6 py-4',
      body: 'px-6 py-5',
      footer: 'border-t border-[color:var(--color-hairline)] px-6 py-3'
    }"
  >
    <template #header>
      <div class="flex items-start gap-3">
        <div
          class="size-8 rounded-md grid place-items-center hairline"
          :class="active.tone === 'green' ? 'pastel-green' : active.tone === 'red' ? 'pastel-red' : 'bg-white'"
        >
          <UIcon
            :name="
              active.direction === 'long'
                ? 'i-lucide-arrow-up-right'
                : active.direction === 'short'
                  ? 'i-lucide-arrow-down-right'
                  : 'i-lucide-minus'
            "
            class="size-4"
          />
        </div>
        <div class="flex-1 min-w-0">
          <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data">Manual signal</div>
          <div class="font-editorial text-[24px] leading-tight mt-0.5 truncate">Inject on {{ botLabel ?? botId }}</div>
          <div class="text-[12px] text-ink-soft mt-0.5">
            Bypasses the indicator but still runs through intent, risk, execution.
          </div>
        </div>
      </div>
    </template>

    <template #body>
      <div class="space-y-5">
        <div>
          <div class="text-[10.5px] uppercase tracking-[0.16em] text-ink-soft font-data mb-2">Signal</div>
          <div class="grid grid-cols-2 gap-2">
            <button
              v-for="opt in SIGNAL_OPTIONS"
              :key="opt.value"
              type="button"
              class="text-left rounded-md border px-3 py-2.5 transition-all"
              :class="
                signal === opt.value
                  ? 'border-[color:var(--color-accent)] bg-[color:var(--color-accent-softer)]'
                  : 'border-[color:var(--color-hairline)] hover:border-[color:var(--color-hairline-strong)]'
              "
              @click="signal = opt.value"
            >
              <div class="flex items-center gap-2">
                <span
                  class="dot"
                  :class="
                    opt.tone === 'green'
                      ? 'bg-[color:var(--color-pastel-green-ink)]'
                      : opt.tone === 'red'
                        ? 'bg-[color:var(--color-pastel-red-ink)]'
                        : 'bg-[color:var(--color-ink-muted)]'
                  "
                />
                <div class="text-[13px] font-medium">{{ opt.label }}</div>
              </div>
              <div class="text-[11px] text-ink-soft font-data mt-1 uppercase tracking-[0.1em]">
                {{ opt.phase === 'none' ? 'flat' : opt.phase }} · {{ opt.direction }}
              </div>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <UFormField
            label="Price"
            :ui="{ label: 'text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data' }"
          >
            <UInput
              v-model.number="price"
              type="number"
              step="0.0001"
              placeholder="e.g. 123.45"
              class="font-data"
            />
          </UFormField>

          <UFormField
            label="Timestamp"
            :ui="{ label: 'text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data' }"
          >
            <div class="flex gap-2">
              <UInput
                v-model="timestamp"
                type="datetime-local"
                class="flex-1 font-data"
              />
              <UButton
                size="sm"
                color="neutral"
                variant="outline"
                icon="i-lucide-clock"
                aria-label="Now"
                @click="stampNow"
              />
            </div>
          </UFormField>
        </div>

        <UFormField
          v-if="symbols && symbols.length > 1"
          label="Symbol"
          :ui="{ label: 'text-[11px] uppercase tracking-[0.14em] text-ink-soft font-data' }"
        >
          <USelect
            v-model="symbol"
            :items="symbolItems"
            placeholder="Default (primary)"
          />
        </UFormField>

        <div
          v-if="invalid"
          class="rounded-md pastel-yellow px-3 py-2 text-[12px] flex items-center gap-2"
        >
          <UIcon
            name="i-lucide-triangle-alert"
            class="size-3.5"
          />
          {{ invalid }}
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-between w-full gap-3">
        <div class="text-[11px] text-ink-soft font-data uppercase tracking-[0.12em]">
          POST /v1/bots/{{ botId }}/manual-signal
        </div>
        <div class="flex items-center gap-2">
          <UButton
            color="neutral"
            variant="ghost"
            size="sm"
            label="Cancel"
            @click="open = false"
          />
          <UButton
            color="neutral"
            variant="solid"
            size="sm"
            :icon="submitting ? 'i-lucide-loader-2' : 'i-lucide-zap'"
            :loading="submitting"
            :disabled="!!invalid"
            label="Insert signal"
            @click="submit"
          />
        </div>
      </div>
    </template>
  </UModal>
</template>
