/**
 * Provides overlay editor helpers for use widget draft state.
 */

import { useCallback, useRef, useState } from 'react'
import {
  clearLiveWidgetDraft,
  clearLiveWidgetDrafts,
} from './overlayEditorHelpers'

/**
 * Provides widget draft state state and actions.
 * @returns {object} Result produced by the helper.
 */
export default function useWidgetDraftState() {
  const draftWidgetsRef = useRef({})
  const [liveWidgetDrafts, setLiveWidgetDrafts] = useState({})

  const setLiveWidgetDraft = useCallback((widgetId, nextDraft) => {
    draftWidgetsRef.current[widgetId] = nextDraft
    setLiveWidgetDrafts((current) => ({
      ...current,
      [widgetId]: nextDraft,
    }))
  }, [])

  const setLiveWidgetDraftsBatch = useCallback((nextDraftsById) => {
    Object.entries(nextDraftsById).forEach(([widgetId, nextDraft]) => {
      draftWidgetsRef.current[widgetId] = nextDraft
    })

    setLiveWidgetDrafts((current) => ({
      ...current,
      ...nextDraftsById,
    }))
  }, [])

  const clearWidgetDraft = useCallback((widgetId) => {
    clearLiveWidgetDraft(draftWidgetsRef, widgetId)
    setLiveWidgetDrafts((current) => {
      if (!current[widgetId]) {
        return current
      }

      const next = { ...current }
      delete next[widgetId]
      return next
    })
  }, [])

  const clearWidgetDrafts = useCallback((widgetIds) => {
    clearLiveWidgetDrafts(draftWidgetsRef, widgetIds)
    setLiveWidgetDrafts((current) => {
      const next = { ...current }
      let changed = false

      widgetIds.forEach((widgetId) => {
        if (!next[widgetId]) {
          return
        }

        delete next[widgetId]
        changed = true
      })

      return changed ? next : current
    })
  }, [])

  const resetWidgetDrafts = useCallback(() => {
    draftWidgetsRef.current = {}
    setLiveWidgetDrafts({})
  }, [])

  return {
    clearWidgetDraft,
    clearWidgetDrafts,
    draftWidgetsRef,
    liveWidgetDrafts,
    resetWidgetDrafts,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
  }
}
