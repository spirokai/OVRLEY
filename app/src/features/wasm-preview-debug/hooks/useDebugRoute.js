import { useSyncExternalStore } from 'react'
import { WASM_PREVIEW_DEBUG_HASH } from '../data/debugRoute'

const subscribe = (callback) => {
  window.addEventListener('hashchange', callback)
  return () => window.removeEventListener('hashchange', callback)
}
const snapshot = () => window.location.hash === WASM_PREVIEW_DEBUG_HASH

/** @returns {boolean} Whether the development-only renderer route is selected. */
export function useDebugRoute() {
  return useSyncExternalStore(subscribe, snapshot)
}
