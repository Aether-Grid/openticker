<script setup lang="ts" generic="T">
defineProps<{
  columns: Array<{ key: string; label: string; align?: 'left' | 'right' | 'center'; width?: string }>
  rows: T[]
  rowKey?: (row: T) => string
}>()

const emit = defineEmits<{
  rowClick: [row: T]
}>()

defineSlots<{
  cell(props: { row: T; column: { key: string; label: string }; value: unknown }): unknown
  empty(): unknown
}>()

function cellValue(row: T, key: string): unknown {
  return (row as Record<string, unknown>)[key]
}
</script>

<template>
  <div class="surface-card overflow-hidden">
    <div class="overflow-x-auto">
      <table class="w-full text-[13px] border-collapse">
        <thead>
          <tr class="text-left">
            <th
              v-for="col in columns"
              :key="col.key"
              class="px-4 py-2.5 text-[10.5px] uppercase tracking-[0.14em] text-ink-soft font-data font-medium hairline-b"
              :class="{ 'text-right': col.align === 'right', 'text-center': col.align === 'center' }"
              :style="col.width ? { width: col.width } : undefined"
            >
              {{ col.label }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(row, idx) in rows"
            :key="rowKey ? rowKey(row) : idx"
            class="hairline-b last:border-b-0 hover:bg-[color:var(--color-accent-softer)] transition-colors cursor-pointer"
            @click="emit('rowClick', row)"
          >
            <td
              v-for="col in columns"
              :key="col.key"
              class="px-4 py-3 align-middle"
              :class="{ 'text-right': col.align === 'right', 'text-center': col.align === 'center' }"
            >
              <slot
                name="cell"
                :row="row"
                :column="col"
                :value="cellValue(row, col.key)"
              >
                {{ cellValue(row, col.key) ?? '—' }}
              </slot>
            </td>
          </tr>
          <tr v-if="!rows.length">
            <td
              :colspan="columns.length"
              class="px-6 py-12 text-center text-ink-soft text-[13px]"
            >
              <slot name="empty"> No records. </slot>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
