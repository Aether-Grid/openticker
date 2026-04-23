<script setup lang="ts">
import { sparkPoints } from '~/utils/format'

defineOptions({ name: 'AppSparkline' })

const props = withDefaults(
  defineProps<{
    values: number[]
    width?: number
    height?: number
    stroke?: string
    area?: boolean
  }>(),
  {
    width: 120,
    height: 32,
    stroke: 'currentColor',
    area: false
  }
)

const poly = computed(() => sparkPoints(props.values, props.width, props.height))
const areaPath = computed(() => {
  if (!props.area || !props.values || props.values.length < 2) return ''
  const pts = poly.value
  if (!pts) return ''
  return `M0,${props.height} L${pts.replace(/ /g, ' L')} L${props.width},${props.height} Z`
})
const trendUp = computed(() => {
  if (!props.values || props.values.length < 2) return null
  const first = props.values[0]
  const last = props.values[props.values.length - 1]
  if (first === undefined || last === undefined) return null
  return last >= first
})
const tone = computed(() => {
  if (trendUp.value === null) return 'var(--color-ink-soft)'
  return trendUp.value ? 'var(--color-pastel-green-ink)' : 'var(--color-pastel-red-ink)'
})
</script>

<template>
  <svg
    :width="width"
    :height="height"
    :viewBox="`0 0 ${width} ${height}`"
    preserveAspectRatio="none"
    class="block"
  >
    <polyline
      v-if="poly"
      :points="poly"
      fill="none"
      :stroke="tone"
      stroke-width="1.2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <path
      v-if="area && areaPath"
      :d="areaPath"
      :fill="tone"
      fill-opacity="0.08"
      stroke="none"
    />
  </svg>
</template>
