/**
 * Implements API helpers for activity cache.
 */

let currentParsedActivity = null

/**
 * Sets current activity cache.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} debugPayload - Value for debug payload.
 * @returns {*} Result produced by the helper.
 */
export function setCurrentActivityCache(activity) {
  currentParsedActivity = activity
}

/**
 * Returns current parsed activity.
 * @returns {*} Requested value or structure.
 */
export function getCurrentParsedActivity() {
  return currentParsedActivity
}

/**
 * Clears current activity cache.
 * @returns {*} Result produced by the helper.
 */
export function clearCurrentActivityCache() {
  currentParsedActivity = null
}

if (import.meta.env.DEV && typeof window !== 'undefined') {
  window.setCurrentActivityCache = setCurrentActivityCache
}
