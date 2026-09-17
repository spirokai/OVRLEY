/**
 * Returns the score color on a red-to-green scale.
 *
 * @param {number|null} score Match score from 0 to 100, or null for map-only candidates.
 * @returns {string|null} CSS HSL color or null when no score is available.
 */
export function getVideoSyncMatchScoreColor(score) {
  if (score === null) return null
  return `hsl(${score * 1.2} 72% 48%)`
}
