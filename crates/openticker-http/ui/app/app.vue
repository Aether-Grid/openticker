<script setup lang="ts">
const title = 'OpenTicker — Control Workspace'
const description =
  'Local control plane for algorithmic trading bots. Monitor cycles, manage capital, and inspect live market data.'

useSeoMeta({
  title,
  description,
  ogTitle: title,
  ogDescription: description,
  twitterCard: 'summary'
})
</script>

<template>
  <UApp>
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </UApp>
</template>
