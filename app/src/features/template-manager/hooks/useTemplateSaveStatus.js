/**
 * Tracks template save status by comparing current editor state against
 * the last-saved snapshot. Returns derived dirty/draft/saved status.
 */

import { useMemo } from 'react'
import { createTemplateState, templateStatesEqual } from '@/lib/template-snapshot'

/**
 * Derives the template save status from current editor state and the
 * last-saved snapshot. Encapsulates the comparison logic that determines
 * whether the template is in Draft, Saved, or Modified state.
 *
 * @param {object} params
 * @param {object|null} params.config - Current editor config.
 * @param {object|null} params.globalDefaults - Current global defaults.
 * @param {number} params.updateRate - Current update rate (FPS).
 * @param {object} params.exportRange - Current export range settings.
 * @param {string} params.exportCodec - Current export codec.
 * @param {string} params.aspectRatio - Current aspect ratio.
 * @param {object|null} params.lastSavedTemplateState - Snapshot from the last save operation.
 * @returns {{ currentTemplateState: object|null, status: string|null, showTemplateStatus: boolean }}
 */
export function useTemplateSaveStatus({ config, globalDefaults, updateRate, exportRange, exportCodec, aspectRatio, lastSavedTemplateState }) {
  const currentTemplateState = useMemo(
    () =>
      createTemplateState({
        config,
        globalDefaults,
        updateRate,
        exportRange,
        exportCodec,
        aspectRatio,
      }),
    [config, globalDefaults, updateRate, exportRange, exportCodec, aspectRatio],
  )

  const status = useMemo(() => {
    if (!config) {
      return null
    }

    if (!lastSavedTemplateState) {
      return 'Draft'
    }

    return templateStatesEqual(currentTemplateState, lastSavedTemplateState) ? 'Saved' : 'Modified'
  }, [config, currentTemplateState, lastSavedTemplateState])

  const showTemplateStatus = status === 'Draft' || status === 'Modified'

  return {
    currentTemplateState,
    status,
    showTemplateStatus,
  }
}
