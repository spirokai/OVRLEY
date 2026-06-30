/**
 * Zustand store — combines all feature slices with Immer, devtools, and subscription middleware.
 */

import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'
import { devtools, subscribeWithSelector } from 'zustand/middleware'
import { createEditorSlice } from './slices/createEditorSlice'
import { createMediaSlice } from './slices/createMediaSlice'
import { createTemplateSlice } from './slices/createTemplateSlice'
import { createVideoImportSlice } from './slices/createVideoImportSlice'
import { createLayoutSlice } from './slices/createLayoutSlice'

function createStoreState(set, get) {
  return {
    ...createTemplateSlice(set, get),
    ...createEditorSlice(set, get),
    ...createMediaSlice(set, get),
    ...createVideoImportSlice(set, get),
    ...createLayoutSlice(set, get),
  }
}

function shouldEnableStoreDevtools() {
  return import.meta.env.DEV && typeof window !== 'undefined'
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
  window.__OVRLEY_STORE__ = useStore
}

export default useStore
