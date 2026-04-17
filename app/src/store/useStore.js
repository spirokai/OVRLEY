import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'
import { devtools } from 'zustand/middleware'
import { createEditorSlice } from './slices/createEditorSlice'
import { createMediaSlice } from './slices/createMediaSlice'
import { createTemplateSlice } from './slices/createTemplateSlice'
export { isUpdatingFromTimelineFlag } from './store-utils'

const useStore = create(
  devtools(
    immer((set, get) => ({
      ...createTemplateSlice(set, get),
      ...createEditorSlice(set, get),
      ...createMediaSlice(set, get),
    })),
    {
      name: 'CyclemetryStore',
      serialize: {
        replacer: (key, value) =>
          key === 'editor' ? '<<MonacoEditor>>' : value,
      },
    },
  ),
)

export default useStore
