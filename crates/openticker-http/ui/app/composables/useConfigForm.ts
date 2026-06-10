import type { ComputedRef, Ref } from 'vue'
import type { ConfigSaveResult, ConfigViolation } from '~/types/api'
import type { FieldErrors } from '~/utils/configValidation'

export interface UseConfigFormOptions<T> {
  /** Loads the canonical entity from the server (reload-after-save source). */
  load: () => Promise<T | null | undefined>
  /** Persists the draft; receives the generation observed at the last load. */
  save: (draft: T, generation?: number) => Promise<ConfigSaveResult>
  /** Optional client-side structural validation run before save(). */
  validate?: (draft: T) => FieldErrors
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
  load: () => Promise<void>
  discard: () => void
  submit: () => Promise<boolean>
  errorFor: (path: string) => string | undefined
  onGenerationChange: (generation: number) => void
}

/**
 * Maps a backend ConfigViolation to a draft field path when the code clearly
 * targets one, otherwise returns null (rendered as a banner). Coarse scopes
 * (`global`, `bot:<id>`, `account:<id>`) never map to a field by themselves.
 */
function violationFieldPath(violation: ConfigViolation): string | null {
  switch (violation.code) {
    case 'timeframe_changed_running':
      return 'timeframe'
    case 'symbols_changed_running':
      return 'symbols'
    case 'storage_changed':
      return 'storage'
    case 'bot_dir_changed':
      return 'service.bot_dir'
    default:
      return null
  }
}

/**
 * Generic config form engine: clone-edit-validate-submit with reload-after-save
 * and never optimistic. `draft` is a structuredClone of `original`; dirtiness
 * is structural via deepEqual. On submit, client validate() runs first and
 * aborts on any error; on a 422/409 the returned violations are mapped onto
 * field errors (unmappable ones become a banner). A successful write always
 * re-loads from the server rather than trusting the local draft.
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

  const discard = () => {
    draft.value = original.value === null ? null : structuredClone(original.value)
    stale.value = false
    conflict.value = false
    clearErrors()
  }

  const applyViolations = (violations: ConfigViolation[]) => {
    const mapped: FieldErrors = {}
    const unmapped: ConfigViolation[] = []
    for (const violation of violations) {
      const path = violationFieldPath(violation)
      if (path && !(path in mapped)) mapped[path] = violation.message
      else if (!path) unmapped.push(violation)
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
        if (unmappedViolations.value.length > 0) {
          generalError.value = result.error ?? unmappedViolations.value.map((v) => v.message).join('; ')
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

  const onGenerationChange = (generation: number) => {
    // Already aligned with what we last loaded: nothing to do.
    if (generation === loadedGeneration.value) return
    if (!dirty.value) {
      // Safe to silently refresh to the new server state, then adopt its
      // generation as our If-Match token for the next write.
      void load().then(() => {
        loadedGeneration.value = generation
      })
    } else {
      // Local edits in flight: warn rather than clobber the user's work.
      stale.value = true
    }
  }

  // Track the generation observed at each load so submit can send If-Match. If
  // the loaded entity does not itself carry `generation`, the page is expected
  // to feed it via onGenerationChange.
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
    load,
    discard,
    submit,
    errorFor,
    onGenerationChange
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
