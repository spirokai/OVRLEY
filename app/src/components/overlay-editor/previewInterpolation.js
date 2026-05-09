/**
 * Provides overlay editor helpers for preview timing.
 */

/**
 * Returns effective preview fps.
 *
 * @param {*} fps - Numeric fps value.
 * @param {*} updateRate - Metric sampling rate used during export.
 * @returns {*} Requested value or structure.
 */
export function getEffectivePreviewFps(fps, updateRate) {
  const safeSceneFps = Math.max(Number(fps) || 30, 1)
  const safeUpdateRate = Math.max(Number(updateRate) || 1, 1)

  return Math.max(safeSceneFps / safeUpdateRate, 1)
}
