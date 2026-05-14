/**
 * Backend debug state management utilities.
 * Manages the window.__BACKEND_DEBUG__ object for tracking backend connection state and logs.
 */

/**
 * Ensures the backend debug state object exists on window.
 * @returns {object|null} The debug state object, or null if window is not available.
 */
function ensureBackendDebugState() {
  if (typeof window === 'undefined') {
    return null
  }

  if (!window.__BACKEND_DEBUG__) {
    window.__BACKEND_DEBUG__ = {
      status: 'initializing',
      error: null,
      logs: [],
      startTime: null,
    }
  }

  return window.__BACKEND_DEBUG__
}

/**
 * Logs a backend-related message to the console and to the debug state.
 * Maintains a rolling buffer of up to 50 log entries.
 * @param {string} message - The message to log.
 */
function logBackend(message) {
  const debugState = ensureBackendDebugState()
  const timestamp = new Date().toISOString()

  console.log(`[Backend] ${message}`)

  if (!debugState) {
    return
  }

  debugState.logs.push(`[${timestamp}] ${message}`)
  if (debugState.logs.length > 50) {
    debugState.logs.shift()
  }
}

/**
 * Updates the current backend status and error in the debug state.
 * @param {string} status - Current status value (e.g. 'connecting', 'connected', 'error').
 * @param {Error|string|null} [error=null] - Optional error associated with the status.
 */
function updateBackendStatus(status, error = null) {
  const debugState = ensureBackendDebugState()
  if (!debugState) {
    return
  }

  debugState.status = status
  debugState.error = error
}

/**
 * Checks whether the code is running in a Tauri runtime environment.
 * @returns {boolean} True if running inside Tauri (window.__TAURI_INTERNALS__ is defined).
 */
export function hasTauriRuntime() {
  return typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'
}

export { ensureBackendDebugState, logBackend, updateBackendStatus }
