import type { NitroFetchRequest, NitroFetchOptions } from 'nitropack/types'

export function useApi() {
  const config = useRuntimeConfig()
  const base = config.public.apiBase || ''

  const api = <T = unknown>(path: NitroFetchRequest, opts?: NitroFetchOptions<NitroFetchRequest>) => {
    const url = typeof path === 'string' ? `${base}${path}` : path
    return $fetch<T>(url as NitroFetchRequest, {
      ...opts,
      headers: {
        Accept: 'application/json',
        ...(opts?.headers ?? {})
      }
    })
  }

  return { api, base }
}
