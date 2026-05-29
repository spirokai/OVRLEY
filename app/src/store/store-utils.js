/**
 * Shared store utilities.
 *
 * Cloning and structural comparison helpers use purpose-built mechanisms
 * (structuredClone for cloning, deep field-by-field traversal for equality)
 * instead of JSON.stringify-based workarounds. Synchronization helpers make
 * config-originated versus timeline-originated timing updates explicit so
 * store actions do not depend on hidden module state or timer windows.
 */

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
 * Applies scene timing from a config-originated update into timeline state.
 *
 * Config replacement and template hydration both flow through this helper.
 * Callers choose whether the playhead should snap back to the new scene start;
 * template hydration does, ordinary config replacement only does so when the
 * incoming scene start actually changes and the playhead is still anchored to
 * the previous start.
 *
 * @param {*} state - Current store draft state.
 * @param {*} nextConfig - Incoming config value that owns the scene timing.
 * @param {object} [options] - Synchronization options.
 * @param {*} [options.previousConfig] - Config value before the current replacement.
 * @param {boolean} [options.resetSelectedSecond=false] - Whether the playhead should adopt the incoming start second immediately.
 */
export function applyConfigOriginatedSceneTiming(state, nextConfig, options = {}) {
  const { previousConfig, resetSelectedSecond = false } = options
  const scene = nextConfig?.scene
  if (!scene) return

  const previousScene = previousConfig?.scene
  const previousStartSecond = state.startSecond
  const previousEndSecond = state.endSecond
  const previousSelectedSecond = state.selectedSecond
  const timelineIsUntouched = previousStartSecond === 0 && previousEndSecond === state.dummyDurationSeconds && previousSelectedSecond === 0

  const sceneStartChangedInConfig = scene.start !== previousScene?.start
  const sceneEndChangedInConfig = scene.end !== previousScene?.end

  if (scene.start !== undefined && (resetSelectedSecond || sceneStartChangedInConfig)) {
    state.startSecond = scene.start

    const playheadIsAnchoredToPreviousStart = previousSelectedSecond === previousStartSecond

    if (resetSelectedSecond || (sceneStartChangedInConfig && (timelineIsUntouched || playheadIsAnchoredToPreviousStart))) {
      state.selectedSecond = scene.start
    }
  }

  if (scene.end !== undefined && (resetSelectedSecond || sceneEndChangedInConfig)) {
    state.endSecond = scene.end
  }
}

/**
 * Applies timeline-originated edits back into config scene timing.
 *
 * Timeline edits are the source of truth for this path. The helper writes the
 * changed bounds into config and refreshes dirty-state tracking if any config
 * field actually changed.
 *
 * @param {*} state - Current store draft state.
 * @param {object} timing - Timeline timing values to persist into config.
 * @param {number} [timing.startSecond] - Updated timeline start second.
 * @param {number} [timing.endSecond] - Updated timeline end second.
 */
export function applyTimelineOriginatedSceneTiming(state, timing) {
  if (!state.config?.scene) return

  let sceneChanged = false

  if (timing.startSecond !== undefined && state.config.scene.start !== timing.startSecond) {
    state.config.scene.start = timing.startSecond
    sceneChanged = true
  }

  if (timing.endSecond !== undefined && state.config.scene.end !== timing.endSecond) {
    state.config.scene.end = timing.endSecond
    sceneChanged = true
  }

  if (sceneChanged) {
    updateConfigPersistence(state)
  }
}
