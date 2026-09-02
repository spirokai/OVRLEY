import { temporal } from 'zundo'
import { immer } from 'zustand/middleware/immer'
import { deepEqual, hasSerializableChanged } from '@/store/store-utils'

const HISTORY_LIMIT = 20

/**
 * Selects the complete editor state that can change an export result.
 *
 * Large immutable inputs such as parsed activity data and imported media
 * metadata are intentionally absent from this projection.
 *
 * @param {object} state - Complete application store state.
 * @returns {object} Undoable editor state.
 */
function partializeUndoableState(state) {
  return {
    config: state.config,
    globalDefaults: state.globalDefaults,
    aspectRatio: state.aspectRatio,
    renderSettings: state.renderSettings,
    videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
    startSecond: state.startSecond,
    endSecond: state.endSecond,
  }
}

/**
 * Creates the immutable application-store initializer with bounded history.
 *
 * @param {Function} initializer - Zustand store initializer.
 * @returns {Function} Initializer enhanced with Immer and Zundo.
 */
export function withEditorHistory(initializer) {
  return temporal(immer(initializer), {
    equality: deepEqual,
    limit: HISTORY_LIMIT,
    partialize: partializeUndoableState,
  })
}

function reconcileRestoredState(store) {
  const state = store.getState()
  if (!state.lastRenderedConfig) return

  const hasUnrenderedChanges = hasSerializableChanged(state.config, state.lastRenderedConfig)
  if (state.hasUnrenderedChanges === hasUnrenderedChanges) return

  store.setState({ hasUnrenderedChanges })
}

/**
 * Restores the previous undoable editor state.
 *
 * @param {object} store - Zustand application store.
 */
export function undoHistory(store) {
  store.temporal.getState().undo()
  reconcileRestoredState(store)
}

/**
 * Restores the next undoable editor state.
 *
 * @param {object} store - Zustand application store.
 */
export function redoHistory(store) {
  store.temporal.getState().redo()
  reconcileRestoredState(store)
}

/**
 * Replaces the current editor document and starts a fresh history.
 *
 * @param {object} store - Zustand application store.
 * @param {Function} operation - Synchronous document replacement.
 * @returns {*} Operation result.
 */
export function replaceEditorDocument(store, operation) {
  const temporalState = store.temporal.getState()
  const shouldResumeTracking = temporalState.isTracking

  if (shouldResumeTracking) {
    temporalState.pause()
  }

  try {
    const result = operation()
    temporalState.clear()
    return result
  } finally {
    if (shouldResumeTracking) {
      temporalState.resume()
    }
  }
}

/**
 * Runs an automatic store transition without adding an undo/redo entry.
 *
 * @param {object} store - Zustand application store.
 * @param {Function} operation - Synchronous or asynchronous transition.
 * @returns {Promise<*>} Operation result.
 */
export async function runWithoutEditorHistory(store, operation) {
  const temporalState = store.temporal.getState()
  if (!temporalState.isTracking) {
    return operation()
  }

  temporalState.pause()
  try {
    return await operation()
  } finally {
    temporalState.resume()
  }
}
