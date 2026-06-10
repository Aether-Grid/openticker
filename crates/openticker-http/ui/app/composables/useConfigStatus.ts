import type { ConfigReloadStatus, ConfigReloadStatusResponse } from '~/types/api'

interface ConfigStatusState {
  generation: number
  last: ConfigReloadStatus | null
  history: ConfigReloadStatus[]
}

/**
 * Shared reload-status state. `pollStatus()` hits
 * `GET /v1/config/reload-status` and updates the reactive generation/last/
 * history. Editor pages can watch `generation` and call
 * `form.onGenerationChange(generation)` to react to out-of-band reloads;
 * `poll` is suitable for passing to useAutoRefresh.
 */
export function useConfigStatus() {
  const { api } = useApi()

  const state = useState<ConfigStatusState>('config-status', () => ({
    generation: 0,
    last: null,
    history: []
  }))

  // Normalized to a message string so consuming pages need not narrow `unknown`.
  const error = useState<string | null>('config-status-error', () => null)

  const generation = computed(() => state.value.generation)
  const last = computed(() => state.value.last)
  const history = computed(() => state.value.history)

  const pollStatus = async () => {
    try {
      const data = await api<ConfigReloadStatusResponse>('/v1/config/reload-status')
      state.value = {
        generation: data.generation,
        last: data.last ?? null,
        history: data.history ?? []
      }
      error.value = null
    } catch (err) {
      // Swallow rather than rethrow: pollStatus is driven by useAutoRefresh,
      // which must not see a thrown error. Normalize to a message string.
      error.value = err instanceof Error ? err.message : String(err)
    }
  }

  return {
    state,
    error,
    generation,
    last,
    history,
    pollStatus,
    poll: pollStatus
  }
}
