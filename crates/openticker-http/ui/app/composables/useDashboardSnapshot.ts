import type { DashboardSnapshot } from '~/types/api'

export function useDashboardSnapshot(limit = 60) {
  const { api } = useApi()
  const snapshot = shallowRef<DashboardSnapshot | null>(null)
  const error = shallowRef<unknown>(null)
  const pending = ref(false)
  const lastLoadedAt = ref<Date | null>(null)

  const load = async () => {
    pending.value = true
    try {
      const data = await api<DashboardSnapshot>(`/v1/dashboard/snapshot?limit=${limit}`)
      snapshot.value = data
      error.value = null
      lastLoadedAt.value = new Date()
    } catch (err) {
      error.value = err
    } finally {
      pending.value = false
    }
  }

  useAutoRefresh(load)
  onMounted(() => {
    void load()
  })

  return { snapshot, error, pending, lastLoadedAt, refresh: load }
}
