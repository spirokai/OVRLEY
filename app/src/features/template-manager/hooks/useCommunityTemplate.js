/**
 * Orchestration hook for loading community templates.
 *
 * Moved out of the Zustand template slice so the store only owns state
 * transitions. The hook handles the network fetch, coordinates cross-slice
 * state updates (demo GPX, duration, timing), syncs the Monaco editor, and
 * surfaces errors to the user.
 *
 * @returns {object} Object containing the selectCommunityTemplate callback.
 */
import { useCallback } from 'react'
import useStore from '@/store/useStore'

/**
 * Hook that provides a selectCommunityTemplate callback for UI components.
 *
 * Callers use this instead of a store action that bundles network I/O,
 * browser UI primitives, and imperative editor manipulation into a single
 * opaque call.
 *
 * @returns {{ selectCommunityTemplate: (filename: string | null) => Promise<void> }} Object with selectCommunityTemplate callback.
 */
export default function useCommunityTemplate() {
  const selectCommunityTemplate = useCallback(async (filename) => {
    const store = useStore.getState()

    store.setLoadedTemplate(null, null)
    store.setCommunityTemplateFilename(filename)

    if (!filename) return

    try {
      const url = `/templates/${filename}`
      const response = await fetch(url)

      if (!response.ok) {
        throw new Error(`Network response was not ok: ${response.status}`)
      }

      const data = await response.json()
      const state = useStore.getState()

      if (!state.activityFilename) {
        useStore.getState().setDemoActivity()
      }

      useStore.getState().setConfig(data)

      const editor = useStore.getState().editor
      if (editor) {
        editor.setValue(data)
      }
    } catch (error) {
      console.error('Error with community templates:', error)
      useStore.getState().setErrorMessage(`Failed to load template: ${error.message}`)
    }
  }, [])

  return { selectCommunityTemplate }
}
