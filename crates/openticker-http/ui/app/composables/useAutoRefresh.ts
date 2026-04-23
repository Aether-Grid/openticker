const DEFAULT_INTERVAL_MS = 5_000

const autoRefreshState = () => {
  return useState<{ enabled: boolean; intervalMs: number }>('auto-refresh', () => ({
    enabled: true,
    intervalMs: DEFAULT_INTERVAL_MS
  }))
}

export function useAutoRefreshSettings() {
  return autoRefreshState()
}

export function useAutoRefresh(callback: () => void | Promise<void>) {
  const state = autoRefreshState()
  let timer: ReturnType<typeof setInterval> | null = null

  const start = () => {
    stop()
    if (!state.value.enabled) return
    timer = setInterval(() => {
      void callback()
    }, state.value.intervalMs)
  }

  const stop = () => {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  watch(() => [state.value.enabled, state.value.intervalMs] as const, start, { immediate: false })

  onMounted(start)
  onBeforeUnmount(stop)

  return { start, stop, state }
}
