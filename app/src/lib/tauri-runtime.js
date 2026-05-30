/**
 * Shared Tauri runtime detection.
 * Returns true when running inside a Tauri desktop shell (IPC available).
 * @returns {boolean}
 */
export function hasTauriRuntime() {
  return typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'
}
