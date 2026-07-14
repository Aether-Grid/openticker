import type { ComputedRef, Ref } from 'vue'
import type { ConfigSaveResult, ConfigViolation } from '~/types/api'
import type { FieldErrors } from '~/utils/configValidation'
import { deepEqual } from '~/utils/deepEqual'

export interface UseConfigFormOptions<T> {
  /** Loads the canonical entity from the server (reload-after-save source). */
  load: () => Promise<T | null | undefined>
  /** Persists the draft; receives the generation observed at the last load. */
  save: (draft: T, generation?: number) => Promise<ConfigSaveResult>
  /** Optional client-side structural validation run before save(). */
  validate?: (draft: T) => FieldErrors
  /**
   * This form's entity scope, used to decide whether a server 409 violation's
   * `scope` matches this page so its message can land on an inline field
   * instead of the banner. Pass `"global"`, `"account:<id>"`, or `"bot:<id>"`.
   * Omit it when the entity has no change-set scope (e.g. risk profiles): with
   * no configured scope every violation falls through to the banner. May be a
   * getter so a page can compute it from a reactive id.
   */
  scope?: string | (() => string)
}

export interface UseConfigForm<T> {
  original: Ref<T | null>
  draft: Ref<T | null>
  dirty: ComputedRef<boolean>
  pending: Ref<boolean>
  saving: Ref<boolean>
  stale: Ref<boolean>
  conflict: Ref<boolean>
  fieldErrors: Ref<FieldErrors>
  unmappedViolations: Ref<ConfigViolation[]>
  generalError: Ref<string | null>
  /** True when any inline field error, unmapped violation, or general error is present. */
  hasErrors: ComputedRef<boolean>
  load: () => Promise<void>
  discard: () => void
  submit: () => Promise<boolean>
  errorFor: (path: string) => string | undefined
  /**
   * Authoritatively stamps the generation that will be sent as the `If-Match`
   * header on the next save. Callers use this to record the server generation
   * observed at the most recent successful load. Set unconditionally: the
   * caller (see `useConfigEditor`) is responsible for only stamping after a
   * load has resolved, so this is race-free with respect to in-flight loads.
   */
  setGeneration: (generation: number) => void
}

/**
 * Maps a backend ConfigViolation code to a draft field path, paired with the
 * coarse scope category (`'global' | 'account' | 'bot'`) the code originates
 * from. A code only maps to an inline field when the form's configured scope
 * also matches this category (see `applyViolations`); otherwise the violation
 * is surfaced as a banner so it is never silently dropped. Codes with no
 * field-level target (e.g. `account_set_changed`, `bindings_changed_running`)
 * are absent here and always render as a banner.
 */
interface FieldMapping {
  path: string
  scopeKind: 'global' | 'account' | 'bot'
}

function violationFieldMapping(violation: ConfigViolation): FieldMapping | null {
  switch (violation.code) {
    case 'timeframe_changed_running':
      return { path: 'timeframe', scopeKind: 'bot' }
    case 'symbols_changed_running':
      return { path: 'symbols', scopeKind: 'bot' }
    case 'storage_changed':
      return { path: 'storage', scopeKind: 'global' }
    case 'bot_dir_changed':
      return { path: 'service.bot_dir', scopeKind: 'global' }
    default:
      return null
  }
}

/** The coarse scope category of a violation `scope` string, or null if unknown. */
function scopeKindOf(scope: string): 'global' | 'account' | 'bot' | null {
  if (scope === 'global') return 'global'
  if (scope.startsWith('account:')) return 'account'
  if (scope.startsWith('bot:')) return 'bot'
  return null
}

/**
 * Generic config form engine: clone-edit-validate-submit with reload-after-save
 * and never optimistic. `draft` is a structuredClone of `original`; dirtiness
 * is structural via deepEqual. A successful write always re-loads from the
 * server rather than trusting the local draft.
 *
 * Error surfaces, in increasing distance from the user's edit:
 *  - Client `validate()` runs first on submit and aborts on any error,
 *    producing inline `fieldErrors` keyed by draft field path.
 *  - A server 409 carries `violations` (the change-set / stale_generation /
 *    duplicate_bot conflicts). Each violation lands inline when its code maps
 *    to a field AND its `scope` matches this form's configured scope; otherwise
 *    it is shown in the banner (`unmappedViolations`). Every violation lands in
 *    exactly one place — inline or banner — never neither.
 *  - A server 422 is a plain `{error}` with NO violations; it becomes a single
 *    `generalError` banner only.
 */
export function useConfigForm<T>(options: UseConfigFormOptions<T>): UseConfigForm<T> {
  const original = ref<T | null>(null) as Ref<T | null>
  const draft = ref<T | null>(null) as Ref<T | null>
  const loadedGeneration = ref<number | undefined>(undefined)

  const pending = ref(false)
  const saving = ref(false)
  const stale = ref(false)
  const conflict = ref(false)
  const fieldErrors = ref<FieldErrors>({})
  const unmappedViolations = ref<ConfigViolation[]>([])
  const generalError = ref<string | null>(null)

  const dirty = computed(() => {
    if (original.value === null || draft.value === null) return false
    return !deepEqual(original.value, draft.value)
  })

  const hasErrors = computed(
    () =>
      Object.keys(fieldErrors.value).length > 0 ||
      unmappedViolations.value.length > 0 ||
      generalError.value !== null
  )

  /** Resolves the form's configured scope (literal or getter), if any. */
  const resolveScope = (): string | undefined =>
    typeof options.scope === 'function' ? options.scope() : options.scope

  const clearErrors = () => {
    fieldErrors.value = {}
    unmappedViolations.value = []
    generalError.value = null
  }

  const load = async () => {
    pending.value = true
    try {
      const data = await options.load()
      const value = (data ?? null) as T | null
      original.value = value
      draft.value = value === null ? null : structuredClone(value)
      stale.value = false
      conflict.value = false
      clearErrors()
    } finally {
      pending.value = false
    }
  }

  // Reverts the draft to the last-loaded `original` (keeping its known
  // generation). It does NOT resolve staleness against the server: only load()
  // fetches the latest server state. Clearing `stale` here just dismisses the
  // warning for the now-reverted draft.
  const discard = () => {
    draft.value = original.value === null ? null : structuredClone(original.value)
    stale.value = false
    conflict.value = false
    clearErrors()
  }

  /**
   * Routes each server violation to exactly one surface: an inline field error
   * when its code maps to a field AND this form has a configured scope that
   * matches the violation's scope; otherwise the banner. Mapping a field always
   * requires a configured scope, so a form with no scope sends everything to
   * the banner. This guarantees no violation is silently lost into a field path
   * the current page does not even render.
   */
  const applyViolations = (violations: ConfigViolation[]) => {
    const formScope = resolveScope()
    const formScopeKind = formScope ? scopeKindOf(formScope) : null
    const mapped: FieldErrors = {}
    const unmapped: ConfigViolation[] = []
    for (const violation of violations) {
      const mapping = violationFieldMapping(violation)
      // Inline only when the code targets a field AND the violation's own scope
      // matches the scope this form is editing. A mismatch (e.g. a bot-scoped
      // violation arriving on the global page) routes to the banner so it is
      // never written to a field the page does not render.
      const scopeMatches =
        mapping !== null &&
        formScope !== undefined &&
        violation.scope === formScope &&
        mapping.scopeKind === formScopeKind
      if (mapping && scopeMatches && !(mapping.path in mapped)) {
        mapped[mapping.path] = violation.message
      } else {
        unmapped.push(violation)
      }
    }
    fieldErrors.value = { ...fieldErrors.value, ...mapped }
    unmappedViolations.value = unmapped
  }

  const submit = async (): Promise<boolean> => {
    if (draft.value === null) return false
    clearErrors()

    if (options.validate) {
      const errors = options.validate(draft.value)
      if (Object.keys(errors).length > 0) {
        fieldErrors.value = errors
        return false
      }
    }

    saving.value = true
    try {
      const result = await options.save(draft.value, loadedGeneration.value)
      if (result.ok) {
        // Reload-after-save: never trust the local draft, re-fetch canonical state.
        await load()
        return true
      }

      conflict.value = result.status === 409
      if (result.violations && result.violations.length > 0) {
        applyViolations(result.violations)
        const landedInline = Object.keys(fieldErrors.value).length > 0
        const landedInBanner = unmappedViolations.value.length > 0
        if (landedInBanner) {
          // Banner already lists the unmapped violations; lift the server error
          // (or their messages) into generalError so it renders as a heading.
          generalError.value = result.error ?? unmappedViolations.value.map((v) => v.message).join('; ')
        } else if (!landedInline) {
          // Defense-in-depth: violations were returned but none landed inline or
          // in the banner. Never swallow them — surface the server error.
          generalError.value = result.error ?? 'Save failed'
        }
      } else {
        generalError.value = result.error ?? 'Save failed'
      }
      return false
    } finally {
      saving.value = false
    }
  }

  const errorFor = (path: string): string | undefined => fieldErrors.value[path]

  // Authoritative If-Match stamp. The orchestrator (useConfigEditor) calls this
  // with the server generation observed at the most recent successful load, so
  // submit() always carries the generation that matches the data the user is
  // editing. Set unconditionally — the orchestrator owns the ordering.
  const setGeneration = (generation: number) => {
    loadedGeneration.value = generation
  }

  // Track the generation off the loaded entity when it carries one (e.g. a
  // future read shape with an embedded `generation`). Entities such as the
  // per-account / per-risk-profile views extracted from EffectiveConfig do NOT
  // carry a `generation` field, so this watch never fires for them; those pages
  // must stamp the generation explicitly via setGeneration() (see
  // useConfigEditor), which is the authoritative path.
  watch(original, () => {
    const fromEntity = readGeneration(original.value)
    if (fromEntity !== undefined) loadedGeneration.value = fromEntity
  })

  return {
    original,
    draft,
    dirty,
    pending,
    saving,
    stale,
    conflict,
    fieldErrors,
    unmappedViolations,
    generalError,
    hasErrors,
    load,
    discard,
    submit,
    errorFor,
    setGeneration
  }
}

/** Reads an optional `generation` field off the loaded entity, when present. */
function readGeneration(value: unknown): number | undefined {
  if (value && typeof value === 'object' && 'generation' in value) {
    const gen = (value as { generation?: unknown }).generation
    if (typeof gen === 'number') return gen
  }
  return undefined
}
