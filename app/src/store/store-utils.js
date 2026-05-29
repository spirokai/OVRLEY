/**
 * Shared store utilities.
 *
 * Cloning and structural comparison helpers use purpose-built mechanisms
 * (structuredClone for cloning, deep field-by-field traversal for equality)
 * instead of JSON.stringify-based workarounds. This makes intent explicit,
 * avoids hidden serialization overhead, and does not depend on property
 * ordering.
 */

let isUpdatingFromConfig = false

export const DEFAULT_CONFIG = {
  scene: {
    width: 1920,
    height: 1080,
    fps: 30,
    start: 0,
    end: 60,
    font: 'Arial.ttf',
    color: '#ffffff',
    font_size: 30,
  },
  labels: [],
  values: [],
  plots: [],
}

export const DEFAULT_RENDER_PROGRESS = {
  renderId: null,
  current: 0,
  total: 0,
  percent: 0,
  status: 'idle',
  message: '',
  estimatedSecondsRemaining: null,
  renderingFps: null,
  encoded: 0,
  filename: null,
}

/**
 * Performs structural deep equality comparison on plain serializable values.
 *
 * Traverses objects and arrays field-by-field. Does not depend on property
 * ordering or JSON-serialization quirks. Handles the same shapes as the
 * previous JSON.stringify-based approach but avoids encoding overhead and
 * implicit assumptions about serializability.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {boolean} Whether the values are structurally equivalent.
 */
export function deepEqual(left, right) {
  if (left === right) return true
  if (left === null || right === null) return left === right
  if (typeof left !== typeof right) return false
  if (typeof left !== 'object') return left === right

  if (Array.isArray(left) !== Array.isArray(right)) return false

  if (Array.isArray(left)) {
    if (left.length !== right.length) return false
    return left.every((item, i) => deepEqual(item, right[i]))
  }

  const keysA = Object.keys(left)
  const keysB = Object.keys(right)
  if (keysA.length !== keysB.length) return false
  return keysA.every((key) => Object.prototype.hasOwnProperty.call(right, key) && deepEqual(left[key], right[key]))
}

/**
 * Clones a plain serializable value using the platform structuredClone API.
 *
 * More efficient than JSON round-tripping and preserves values that JSON
 * cannot represent (undefined, NaN, etc.). The returned clone is a complete
 * deep copy with no shared references to the source.
 *
 * @param {*} value - Value to clone.
 * @returns {*} Deep clone of the input value.
 */
export function cloneSerializable(value) {
  return structuredClone(value)
}

/**
 * Checks whether two plain serializable values differ structurally.
 *
 * Builds on deepEqual so the comparison is semantic rather than
 * stringify-based. Any remaining callers that pass non-equal-able values
 * (functions, symbols) will fall through to the strict-inequality path
 * inside deepEqual.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {boolean} Whether the values differ.
 */
export function hasSerializableChanged(left, right) {
  return !deepEqual(left, right)
}

/**
 * Updates derived dirty state after config changes.
 *
 * @param {*} state - Current store draft state.
 */
export function updateConfigPersistence(state) {
  if (state.lastRenderedConfig) {
    state.hasUnrenderedChanges = hasSerializableChanged(state.config, state.lastRenderedConfig)
    return
  }

  state.hasUnrenderedChanges = true
}

/**
 * Marks a config-driven update as in progress.
 *
 * @returns {boolean} Whether a config update was already in progress.
 */
export function beginConfigUpdate() {
  const wasUpdating = isUpdatingFromConfig
  isUpdatingFromConfig = true
  return wasUpdating
}

/**
 * Clears the config-update guard after the current tick settles.
 */
export function endConfigUpdateSoon() {
  setTimeout(() => {
    isUpdatingFromConfig = false
  }, 100)
}

/**
 * Checks whether a config update is currently in progress.
 *
 * @returns {boolean} Whether config synchronization is active.
 */
export function isConfigUpdateInProgress() {
  return isUpdatingFromConfig
}
