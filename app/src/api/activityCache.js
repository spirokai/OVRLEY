/**
 * Implements API helpers for activity cache.
 */

let currentParsedActivity = null
let currentParsedActivityDebug = null

/**
 * Sets current activity cache.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} debugPayload - Value for debug payload.
 * @returns {*} Result produced by the helper.
 */
export function setCurrentActivityCache(activity, debugPayload = null) {
  currentParsedActivity = activity
  currentParsedActivityDebug = debugPayload
}

/**
 * Returns current parsed activity.
 * @returns {*} Requested value or structure.
 */
export function getCurrentParsedActivity() {
  return currentParsedActivity
}

/**
 * Returns current parsed activity debug.
 * @returns {*} Requested value or structure.
 */
export function getCurrentParsedActivityDebug() {
  return currentParsedActivityDebug
}

/**
 * Clears current activity cache.
 * @returns {*} Result produced by the helper.
 */
export function clearCurrentActivityCache() {
  currentParsedActivity = null
  currentParsedActivityDebug = null
}
