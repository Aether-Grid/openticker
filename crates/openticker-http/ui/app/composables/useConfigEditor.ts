import type { ComputedRef } from 'vue'
import type { ConfigSaveResult } from '~/types/api'
import type { FieldErrors } from '~/utils/configValidation'
import type { UseConfigForm } from '~/composables/useConfigForm'

export interface UseConfigEditorOptions<T> {
  /** Loads the canonical entity from the server (reload-after-save source). */
  load: () => Promise<T | null | undefined>
  /** Persists the draft; receives the generation observed at the last load. */
  save: (draft: T, generation?: number) => Promise<ConfigSaveResult>
  /** Optional client-side structural validation run before save(). */
  validate?: (draft: T) => FieldErrors
  /**
   * This form's entity scope, used to attribute server 409 violations to inline
   * fields. See `useConfigForm` for the contract; forwarded verbatim.
   */
  scope?: string | (() => string)
}

export interface UseConfigEditor<T> {
  /** The underlying clone-edit-validate-submit form engine. */
  form: UseConfigForm<T>
  /**
   * True once the first load has completed AND found no entity. Distinct from
   * `form.pending`: it stays false until the first reload resolves, so the page
   * never flashes an EmptyState before the initial fetch.
   */
  notFound: ComputedRef<boolean>
  /**
   * Loads the entity and authoritatively re-stamps the If-Match generation from
   * the latest polled reload-status. Stamping happens AFTER the load resolves,
   * so it is race-free with respect to in-flight loads.
   */
  reload: () => Promise<void>
}

/**
 * Per-page wiring for a single-entity config editor, encapsulated once so each
 * editor page (account, risk profile, and the upcoming global/bots editors)
 * shares the same authoritative-generation and lifecycle behaviour.
 *
 * The authoritative rule it enforces: `loadedGeneration` (the `If-Match` token)
 * always equals the server generation observed at the most recent successful
 * load. The entities these pages edit are extracted from `EffectiveConfig` and
 * carry no `generation` field of their own, so the generation is sourced from
 * the shared reload-status poll and stamped explicitly via `form.setGeneration`
 * AFTER each load resolves.
 *
 * Lifecycle:
 *  - onMounted: poll reload-status first (so the generation is known), then
 *    reload() once. Exactly one initial load.
 *  - watch(generation): on an out-of-band bump, a clean form reloads and
 *    re-stamps; a dirty form raises `stale` rather than clobbering edits.
 *  - useAutoRefresh keeps the reload-status poll alive (NOT the form payload).
 *  - onBeforeRouteLeave confirms discard while dirty.
 */
export function useConfigEditor<T>(options: UseConfigEditorOptions<T>): UseConfigEditor<T> {
  const status = useConfigStatus()

  const form = useConfigForm<T>({
    load: options.load,
    save: options.save,
    validate: options.validate,
    scope: options.scope
  })

  // Flips true only after the FIRST reload completes, so the page can tell
  // "still loading" from "loaded, no such entity" and never flashes EmptyState
  // on the first client render (when pending===false && original===null).
  const loaded = ref(false)

  const notFound = computed(() => loaded.value && form.original.value === null)

  // Load the entity, then stamp the If-Match generation from the latest polled
  // reload-status. Order matters: stamping AFTER load() resolves means the
  // generation we send always matches the data the user is now editing, and it
  // cannot be overwritten by a concurrent in-flight load.
  const reload = async () => {
    await form.load()
    if (status.generation.value != null) form.setGeneration(status.generation.value)
    loaded.value = true
  }

  onMounted(async () => {
    // Poll first so the generation is known before the initial stamp; then do
    // exactly one load. pollStatus swallows its own errors.
    await status.pollStatus()
    await reload()
  })

  // React to out-of-band generation changes observed by the auto-refresh poll.
  // No `immediate`: the onMounted path owns the initial load+stamp.
  watch(
    () => status.generation.value,
    (g) => {
      if (g == null) return
      if (form.dirty.value) {
        // Local edits in flight: warn rather than silently reload and lose them.
        form.stale.value = true
      } else {
        // Clean form: adopt the new server state and re-stamp the generation.
        void reload()
      }
    }
  )

  // Keep the reload-status generation fresh; this drives the watch above. It
  // polls status only — never the form payload.
  useAutoRefresh(status.poll)

  onBeforeRouteLeave(() => {
    if (form.dirty.value && !window.confirm('Discard unsaved changes?')) return false
    return true
  })

  return { form, notFound, reload }
}
