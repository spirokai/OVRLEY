/**
 * Provides overlay editor helpers for preview timing.
 */

import { getContainerFps } from '@/lib/update-rate'

/**
 * Returns effective preview fps.
 *
 * @param {*} fps - Numeric fps value.
 * @param {*} updateRate - Metric sampling rate used during export.
 * @returns {*} Requested value or structure.
 */
export function getEffectivePreviewFps(fps, updateRate) {
  return getContainerFps(fps, updateRate)
}
