/**
 * Provides store utilities related to use store.
 */

import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'
import { devtools, subscribeWithSelector } from 'zustand/middleware'
import { createEditorSlice } from './slices/createEditorSlice'
import { createMediaSlice } from './slices/createMediaSlice'
import { createTemplateSlice } from './slices/createTemplateSlice'
import { createVideoImportSlice } from './slices/createVideoImportSlice'
import { createLayoutSlice } from './slices/createLayoutSlice'

/**
 * Creates store state.
 *
 * @param {*} set - Zustand setter callback.
 * @param {*} get - Value for get.
 * @returns {object} Derived data structure for downstream use.
 */
function createStoreState(set, get) {
  return {
    ...createTemplateSlice(set, get),
    ...createEditorSlice(set, get),
    ...createMediaSlice(set, get),
    ...createVideoImportSlice(set, get),
    ...createLayoutSlice(set, get),
  }
}

/**
 * Checks whether should enable store devtools.
 * @returns {boolean} Whether the condition is satisfied.
 */
function shouldEnableStoreDevtools() {
  if (!import.meta.env.DEV || typeof window === 'undefined') {
    return false
  }

  return window.localStorage.getItem('ovrley:store-devtools') === 'true'
}

const storeInitializer = subscribeWithSelector(immer(createStoreState))

const useStore = create(
  shouldEnableStoreDevtools()
    ? devtools(storeInitializer, {
        name: 'OVRLEYStore',
        serialize: {
          replacer: (key, value) => (key === 'editor' ? '<<MonacoEditor>>' : value),
        },
      })
    : storeInitializer,
)

if (import.meta.env.DEV && typeof window !== 'undefined') {
  window.useStore = useStore
  window.__OVRLEY_STORE__ = useStore
  console.info('[OVRLEY] Store exposed as window.useStore and window.__OVRLEY_STORE__')
}

export default useStore
