/**
 * Zustand store — combines all feature slices with undo history.
 */

import { create } from 'zustand'
import { createEditorSlice } from './slices/createEditorSlice'
import { createMediaSlice } from './slices/createMediaSlice'
import { createTemplateSlice } from './slices/createTemplateSlice'
import { createVideoImportSlice } from './slices/createVideoImportSlice'
import { createLayoutSlice } from './slices/createLayoutSlice'
import { createRenderSettingsSlice } from './slices/createRenderSettingsSlice'
import { withEditorHistory } from '@/features/undo-redo/undoHistory'

function createStoreState(set, get) {
  return {
    ...createTemplateSlice(set, get),
    ...createEditorSlice(set, get),
    ...createMediaSlice(set, get),
    ...createVideoImportSlice(set, get),
    ...createRenderSettingsSlice(set, get),
    ...createLayoutSlice(set, get),
  }
}

const storeInitializer = withEditorHistory(createStoreState)

const useStore = create(storeInitializer)

if (import.meta.env.DEV && typeof window !== 'undefined') {
  window.__OVRLEY_STORE__ = useStore
}

export default useStore
