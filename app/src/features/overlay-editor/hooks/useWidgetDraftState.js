/**
 * Live widget draft state - temporary edits during drag/resize/scale/rotate.
 *
 * Maintains mutable refs for synchronous access during interaction callbacks,
 * and React state mirrors for rendering. Drafts are committed to config only
 * when the interaction ends.
 */

import { useCallback, useRef, useState } from 'react'
import { clearLiveWidgetDraft, clearLiveWidgetDrafts } from '../utils/widgetDomHelpers'

export default function useWidgetDraftState() {
  const draftWidgetsRef = useRef({})
  const [liveWidgetDrafts, setLiveWidgetDrafts] = useState({})
  const widgetPreviewRef = useRef({})
  const [liveWidgetPreviews, setLiveWidgetPreviews] = useState({})

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

  const setLiveWidgetPreview = useCallback((widgetId, nextPreview) => {
    widgetPreviewRef.current[widgetId] = nextPreview
    setLiveWidgetPreviews((current) => ({
      ...current,
      [widgetId]: nextPreview,
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
    widgetPreviewRef.current = {}
    setLiveWidgetPreviews({})
  }, [])

  return {
    clearWidgetDraft,
    clearWidgetDrafts,
    draftWidgetsRef,
    liveWidgetDrafts,
    liveWidgetPreviews,
    resetWidgetDrafts,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
    setLiveWidgetPreview,
  }
}
