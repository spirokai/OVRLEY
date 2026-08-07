/**
 * Calculates the visible updater progress percentage from validated state.
 *
 * @param {object} progress - Canonical download progress state.
 * @returns {number} Progress percentage in the range [0, 100].
 */
export function getUpdateProgressPercent(progress) {
  if (progress.mode === 'indeterminate') {
    return 0
  }
  return Math.min((progress.downloadedBytes / progress.contentLength) * 100, 100)
}

/**
 * Formats validated updater progress for the dialog label.
 *
 * @param {number} progressPercent - Progress percentage.
 * @returns {string} Human-readable progress label.
 */
export function formatUpdateProgress(progressPercent) {
  return `${Math.round(progressPercent)}% downloaded`
}
