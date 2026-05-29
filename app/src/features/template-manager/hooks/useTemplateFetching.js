/**
 * Orchestration hook for fetching the template list from the backend.
 *
 * Moved out of the Zustand template slice so the store only owns state
 * transitions. The hook wraps the network call (backend.listTemplates)
 * and pushes the result into the store via the pure setTemplates action.
 *
 * @returns {object} Object containing the fetchTemplates callback.
 */
import { useCallback } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * Hook that provides a fetchTemplates callback for orchestration layers.
 *
 * Callers such as app bootstrap and the template management hook use this
 * instead of reaching into the store for an action that performs network I/O.
 *
 * @returns {{ fetchTemplates: () => Promise<void> }} Object with fetchTemplates callback.
 */
export default function useTemplateFetching() {
  const fetchTemplates = useCallback(async () => {
    try {
      const templates = await backend.listTemplates()
      useStore.getState().setTemplates(templates)
    } catch (err) {
      console.error('Failed to fetch templates:', err)
    }
  }, [])

  return { fetchTemplates }
}
