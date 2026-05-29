/**
 * Shared store utilities.
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
 * Clones a JSON-serializable value.
 *
 * @param {*} value - Value to clone.
 * @returns {*} Cloned value.
 */
export function cloneSerializable(value) {
  return JSON.parse(JSON.stringify(value))
}

/**
 * Checks whether two JSON-serializable values differ.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {boolean} Whether the values differ.
 */
export function hasSerializableChanged(left, right) {
  return JSON.stringify(left) !== JSON.stringify(right)
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
