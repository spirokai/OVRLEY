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
    videoEdits: state.videoEdits,
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
 * @param {...*} args - Arguments passed to the replacement operation.
 * @returns {*} Operation result.
 */
export function replaceEditorDocument(store, operation, ...args) {
  const temporalState = store.temporal.getState()
  const shouldResumeTracking = temporalState.isTracking

  if (shouldResumeTracking) {
    temporalState.pause()
  }

  try {
    const result = operation(...args)
    store.setState((state) => ({ editorDocumentRevision: state.editorDocumentRevision + 1 }))
    temporalState.clear()
    return result
  } finally {
    if (shouldResumeTracking) {
      temporalState.resume()
    }
  }
}

function rebaseVideoEdits(historicalEdits, currentEdits) {
  return Object.fromEntries(Object.keys(currentEdits).map((videoId) => [videoId, historicalEdits[videoId] ?? currentEdits[videoId]]))
}

/**
 * Replaces or changes the imported-video collection without adding an undo/redo
 * entry. History keeps earlier edits of videos that remain, starts removed videos'
 * edits nowhere, and gives newly added videos their current edits.
 *
 * @param {object} store - Zustand application store.
 * @param {Function} operation - Synchronous collection change.
 * @returns {*} Operation result.
 */
export function changeVideoCollection(store, operation) {
  const result = runWithoutEditorHistory(store, operation)
  const currentEdits = store.getState().videoEdits
  const temporalState = store.temporal.getState()
  const rebase = (historyState) => ({ ...historyState, videoEdits: rebaseVideoEdits(historyState.videoEdits, currentEdits) })
  store.temporal.setState({
    pastStates: temporalState.pastStates.map(rebase),
    futureStates: temporalState.futureStates.map(rebase),
  })
  return result
}

/**
 * Runs an automatic store transition without adding an undo/redo entry.
 *
 * @param {object} store - Zustand application store.
 * @param {Function} operation - Synchronous or asynchronous transition.
 * @returns {*|Promise<*>} Operation result, preserving synchronous transitions.
 */
export function runWithoutEditorHistory(store, operation) {
  const temporalState = store.temporal.getState()
  if (!temporalState.isTracking) {
    return operation()
  }

  temporalState.pause()
  try {
    const result = operation()
    if (result instanceof Promise) return result.finally(() => temporalState.resume())
    temporalState.resume()
    return result
  } catch (error) {
    temporalState.resume()
    throw error
  }
}
