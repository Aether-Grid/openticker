export function useServiceActions() {
  const { api } = useApi()
  const toast = useToast()

  const run = async (label: string, request: () => Promise<unknown>) => {
    try {
      await request()
      toast.add({ title: label, color: 'success', icon: 'i-lucide-check', duration: 2500 })
      return true
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      toast.add({ title: `${label} failed`, description: message, color: 'error', icon: 'i-lucide-triangle-alert' })
      return false
    }
  }

  return {
    reloadConfig: () => run('Config reloaded', () => api('/v1/config/reload', { method: 'POST' })),
    killSwitchOn: () => run('Kill switch engaged', () => api('/v1/risk/kill-switch', { method: 'POST' })),
    killSwitchClear: () => run('Kill switch cleared', () => api('/v1/risk/clear-kill-switch', { method: 'POST' })),
    startBot: (id: string) => run(`Start ${id}`, () => api(`/v1/bots/${id}/start`, { method: 'POST' })),
    stopBot: (id: string) => run(`Stop ${id}`, () => api(`/v1/bots/${id}/stop`, { method: 'POST' })),
    pauseBot: (id: string) => run(`Pause ${id}`, () => api(`/v1/bots/${id}/pause`, { method: 'POST' })),
    resumeBot: (id: string) => run(`Resume ${id}`, () => api(`/v1/bots/${id}/resume`, { method: 'POST' })),
    reconcileBot: (id: string) => run(`Reconcile ${id}`, () => api(`/v1/bots/${id}/reconcile`, { method: 'POST' })),
    tickBot: (id: string) => run(`Tick ${id}`, () => api(`/v1/bots/${id}/tick`, { method: 'POST' })),
    cancelOrders: (id: string) =>
      run(`Cancel orders ${id}`, () => api(`/v1/bots/${id}/cancel-open-orders`, { method: 'POST' })),
    closePositions: (id: string) =>
      run(`Close positions ${id}`, () => api(`/v1/bots/${id}/close-positions`, { method: 'POST' }))
  }
}
