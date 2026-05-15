/**
 * Live widget draft state — temporary edits during drag/resize/scale/rotate.
 *
 * Maintains a mutable ref (draftWidgetsRef) for synchronous access during
 * interaction frame callbacks, and a React state (liveWidgetDrafts) for
 * triggering re-renders. Both are kept in sync.
 *
 * Drafts are committed to config only when the interaction ends
 * (onDragEnd, onResizeEnd, onScaleEnd, onRotateEnd).
 */

import { useCallback, useRef, useState } from 'react'
import { clearLiveWidgetDraft, clearLiveWidgetDrafts } from '../utils/widgetDomHelpers'

/**
 * Provides live widget draft state — temporary unsaved property overrides
 * applied during moveable interactions (drag, resize, scale, rotate).
 *
 * @returns {{
 *   clearWidgetDraft: (widgetId: string) => void,
 *   clearWidgetDrafts: (widgetIds: string[]) => void,
 *   draftWidgetsRef: React.MutableRefObject<Object<string, Object>>,
 *   liveWidgetDrafts: Object<string, Object>,
 *   resetWidgetDrafts: () => void,
 *   setLiveWidgetDraft: (widgetId: string, nextDraft: Object) => void,
 *   setLiveWidgetDraftsBatch: (nextDraftsById: Object<string, Object>) => void,
 * }}
 */
export default function useWidgetDraftState() {
  // Mutable ref and react state — dual storage for sync + render-trigger
  const draftWidgetsRef = useRef({})
  const [liveWidgetDrafts, setLiveWidgetDrafts] = useState({})

  // Draft mutations — set single, set batch, clear single, clear batch, reset all
  const setLiveWidgetDraft = useCallback((widgetId, nextDraft) => {
    draftWidgetsRef.current[widgetId] = nextDraft
    setLiveWidgetDrafts((current) => ({
      ...current,
      [widgetId]: nextDraft,
    }))
  }, [])

  // Batch set — updates multiple widget drafts in a single state commit
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

  // Return — draft state and mutation actions for moveable interaction handlers
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
