/**
 * Formatting utilities for render video display values.
 * Pure functions — no React imports.
 */

/**
 * Formats numeric seconds into a mm:ss display string.
 *
 * @param {number|null|undefined} seconds - Numeric seconds value.
 * @returns {string} Formatted representation (e.g. "5:30").
 */
export function formatTime(seconds) {
  if (seconds === null || seconds === undefined) {
    return '--:--'
  }

  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

/**
 * Formats render production FPS for display.
 *
 * @param {number|null|undefined} fps - Numeric frames-per-second value.
 * @returns {string} Formatted FPS value.
 */
export function formatFps(fps) {
  if (fps === null || fps === undefined || !Number.isFinite(fps)) {
    return '--'
  }

  return fps.toFixed(1)
}
