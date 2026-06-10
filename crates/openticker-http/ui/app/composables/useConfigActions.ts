import type { FetchError } from 'ofetch'
import type { NitroFetchOptions, NitroFetchRequest } from 'nitropack/types'
import type {
  AccountConfigUpdate,
  BotConfig,
  ConfigErrorResponse,
  ConfigReloadStatusResponse,
  ConfigSaveResult,
  ConfigViolation,
  EffectiveConfig,
  GlobalConfig,
  RiskProfileConfig
} from '~/types/api'

/** Statuses returned for inline rendering rather than surfaced as a toast. */
const INLINE_STATUSES = new Set([409, 422])

function isFetchError(err: unknown): err is FetchError {
  return err instanceof Error && 'data' in err
}

/**
 * Config mutation + read wrappers. Mutations never throw: validation (422) and
 * change-set/stale/duplicate conflicts (409) are returned as ConfigSaveResult
 * for inline display. Anything else (404/409-not-inline aside, 500, 501,
 * network) is toasted as an unexpected error and also returned. Successes get a
 * short confirmation toast.
 */
export function useConfigActions() {
  const { api } = useApi()
  const toast = useToast()

  const withIfMatch = (generation: number | undefined, opts: NitroFetchOptions<NitroFetchRequest>) => {
    if (generation === undefined || generation === null) return opts
    return {
      ...opts,
      headers: {
        ...(opts.headers ?? {}),
        'If-Match': String(generation)
      }
    }
  }

  const mutate = async <T>(
    label: string,
    path: string,
    opts: NitroFetchOptions<NitroFetchRequest>,
    entityKey?: string
  ): Promise<ConfigSaveResult<T>> => {
    try {
      const body = await api<Record<string, unknown>>(path, opts)
      toast.add({ title: label, color: 'success', icon: 'i-lucide-check', duration: 2500 })
      return {
        ok: true,
        status: 200,
        generation: typeof body?.generation === 'number' ? body.generation : undefined,
        entity: entityKey ? (body?.[entityKey] as T | undefined) : undefined
      }
    } catch (err) {
      const status = isFetchError(err) ? err.statusCode : undefined
      const data = (isFetchError(err) ? err.data : undefined) as ConfigErrorResponse | undefined
      const message = data?.error ?? (err instanceof Error ? err.message : String(err))
      const violations: ConfigViolation[] | undefined = data?.violations

      // 422/409 are expected validation/conflict outcomes: return for inline
      // rendering and stay quiet. Everything else is unexpected -> toast.
      if (status === undefined || !INLINE_STATUSES.has(status)) {
        toast.add({
          title: `${label} failed`,
          description: message,
          color: 'error',
          icon: 'i-lucide-triangle-alert'
        })
      }
      return { ok: false, status, error: message, violations }
    }
  }

  return {
    fetchEffective: () => api<EffectiveConfig>('/v1/config/effective'),
    fetchReloadStatus: () => api<ConfigReloadStatusResponse>('/v1/config/reload-status'),

    saveGlobal: (draft: GlobalConfig, generation?: number) =>
      mutate<GlobalConfig>(
        'Global config updated',
        '/v1/config/global',
        withIfMatch(generation, { method: 'PUT', body: draft }),
        'global'
      ),

    saveBot: (id: string, draft: BotConfig, generation?: number) =>
      mutate<BotConfig>(
        `Bot ${id} updated`,
        `/v1/config/bots/${id}`,
        withIfMatch(generation, { method: 'PUT', body: draft }),
        'bot'
      ),

    createBot: (draft: BotConfig, generation?: number) =>
      mutate<BotConfig>(
        `Bot ${draft.id} created`,
        '/v1/config/bots',
        withIfMatch(generation, { method: 'POST', body: draft }),
        'bot'
      ),

    deleteBot: (id: string, generation?: number) =>
      mutate(`Bot ${id} deleted`, `/v1/config/bots/${id}`, withIfMatch(generation, { method: 'DELETE' })),

    saveRisk: (id: string, draft: RiskProfileConfig, generation?: number) =>
      mutate<RiskProfileConfig>(
        `Risk profile ${id} updated`,
        `/v1/config/risk-profiles/${id}`,
        withIfMatch(generation, { method: 'PUT', body: draft }),
        'risk_profile'
      ),

    saveAccount: (id: string, draft: AccountConfigUpdate, generation?: number) =>
      mutate(
        `Account ${id} updated`,
        `/v1/config/accounts/${id}`,
        withIfMatch(generation, { method: 'PUT', body: draft }),
        'account'
      )
  }
}
