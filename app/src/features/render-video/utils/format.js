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
