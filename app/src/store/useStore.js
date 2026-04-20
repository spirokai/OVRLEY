import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'
import { devtools } from 'zustand/middleware'
import { createEditorSlice } from './slices/createEditorSlice'
import { createMediaSlice } from './slices/createMediaSlice'
import { createTemplateSlice } from './slices/createTemplateSlice'
export { isUpdatingFromTimelineFlag } from './store-utils'

function createStoreState(set, get) {
  return {
    ...createTemplateSlice(set, get),
    ...createEditorSlice(set, get),
    ...createMediaSlice(set, get),
  }
}

function shouldEnableStoreDevtools() {
  if (!import.meta.env.DEV || typeof window === 'undefined') {
    return false
  }

  return window.localStorage.getItem('cyclemetry:store-devtools') === 'true'
}

const storeInitializer = immer(createStoreState)

const useStore = create(
  shouldEnableStoreDevtools()
    ? devtools(storeInitializer, {
        name: 'CyclemetryStore',
        serialize: {
          replacer: (key, value) =>
            key === 'editor' ? '<<MonacoEditor>>' : value,
        },
      })
    : storeInitializer,
)

export default useStore
