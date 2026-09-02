/**
 * Tracks template save status by comparing current editor state against
 * the last-saved snapshot. Returns derived dirty/draft/saved status.
 */

import { useMemo } from 'react'
import { deepEqual } from '@/store/store-utils'
import { createTemplateState } from '../utils/templateSnapshot'

/**
 * Derives the template save status from current editor state and the
 * last-saved snapshot. Encapsulates the comparison logic that determines
 * whether the template is in Draft, Saved, or Modified state.
 *
 * @param {object} params
 * @param {object|null} params.config - Current editor config.
 * @param {object|null} params.globalDefaults - Current global defaults.
 * @param {object|null} params.lastSavedTemplateState - Snapshot from the last save operation.
 * @returns {{ currentTemplateState: object|null, status: string|null, showTemplateStatus: boolean }}
 */
export function useTemplateSaveStatus({ config, globalDefaults, lastSavedTemplateState }) {
  const currentTemplateState = useMemo(
    () =>
      createTemplateState({
        config,
        globalDefaults,
      }),
    [config, globalDefaults],
  )

  const status = useMemo(() => {
    if (!config) {
      return null
    }

    if (!lastSavedTemplateState) {
      return 'Draft'
    }

    return deepEqual(currentTemplateState, lastSavedTemplateState) ? 'Saved' : 'Modified'
  }, [config, currentTemplateState, lastSavedTemplateState])

  const showTemplateStatus = status === 'Draft' || status === 'Modified'

  return {
    currentTemplateState,
    status,
    showTemplateStatus,
  }
}
