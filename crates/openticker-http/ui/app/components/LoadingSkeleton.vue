<script setup lang="ts">
defineProps<{
  width?: string
  height?: string
  rounded?: string
}>()
</script>

<template>
  <span
    class="skel"
    :style="{
      width: width ?? '100%',
      height: height ?? '1em',
      borderRadius: rounded ?? '6px',
      display: 'inline-block'
    }"
  />
</template>
