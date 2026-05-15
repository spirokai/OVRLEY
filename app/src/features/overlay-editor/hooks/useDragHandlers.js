/**
 * Drag handler group for OverlayMoveable — single-drag and group-drag logic.
 */

import { AXIS_LOCK_THRESHOLD } from '../data/overlayEditorConstants'
import { applyLiveWidgetStyles, getWidgetIdFromTarget } from '../utils/widgetDomHelpers'

function getAxisLockedTranslate(origin, beforeTranslate, inputEvent) {
  if (!inputEvent?.ctrlKey) {
    origin.dragAxisLock = null
    return beforeTranslate
  }

  const translateX = beforeTranslate[0] ?? 0
  const translateY = beforeTranslate[1] ?? 0
  const absX = Math.abs(translateX)
  const absY = Math.abs(translateY)

  if (!origin.dragAxisLock) {
    if (Math.max(absX, absY) < AXIS_LOCK_THRESHOLD) {
      return [0, 0]
    }

    origin.dragAxisLock = absX >= absY ? 'x' : 'y'
  }

  return origin.dragAxisLock === 'x' ? [translateX, 0] : [0, translateY]
}

/**
 * Creates drag-related moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef
 * @param {object} ctx.draftWidgetsRef
 * @param {object} ctx.selectedWidget
 * @param {Array} ctx.selectedWidgets
 * @param {number} ctx.globalScale
 * @param {object} ctx.renderedWidgetMap
 * @param {Array} ctx.effectiveSelectedWidgetIds
 * @param {Array} ctx.groupDragSelectionIds
 * @param {Function} ctx.setLiveWidgetDraft
 * @param {Function} ctx.setLiveWidgetDraftsBatch
 * @param {Function} ctx.commitWidgetUpdate
 * @param {Function} ctx.commitWidgetUpdates
 * @param {Function} ctx.clearWidgetDraft
 * @param {Function} ctx.clearWidgetDrafts
 * @param {Function} ctx.setIsGroupDragActive
 * @param {Function} ctx.setGroupDragSelectionIds
 * @returns {object} Drag handler methods.
 */
export function useDragHandlers({
  interactionStartRef,
  draftWidgetsRef,
  selectedWidget,
  selectedWidgets,
  globalScale,
  renderedWidgetMap,
  effectiveSelectedWidgetIds,
  groupDragSelectionIds,
  setLiveWidgetDraft,
  setLiveWidgetDraftsBatch,
  commitWidgetUpdate,
  commitWidgetUpdates,
  clearWidgetDraft,
  clearWidgetDrafts,
  setIsGroupDragActive,
  setGroupDragSelectionIds,
}) {
  // Drag handlers — single and group drag with axis lock via Ctrl key
  return {
    onDragStart: () => {
      if (!selectedWidget) return

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        type: 'single-drag',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onDrag: ({ beforeTranslate, inputEvent, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return
      const lockedTranslate = getAxisLockedTranslate(origin, beforeTranslate, inputEvent)

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: origin.x + lockedTranslate[0],
        y: origin.y + lockedTranslate[1],
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(target, selectedWidget, nextDraft, globalScale)
    },
    onDragEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onDragGroupStart: () => {
      if (!selectedWidgets.length) return

      const draggedWidgetIds = [...effectiveSelectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      interactionStartRef.current = {
        type: 'group-drag',
        widgetIds: draggedWidgetIds,
        widgetsById: Object.fromEntries(
          selectedWidgets.map((widget) => [
            widget.id,
            {
              x: widget.data.x ?? 0,
              y: widget.data.y ?? 0,
            },
          ]),
        ),
      }

      draggedWidgetIds.forEach((widgetId) => {
        draftWidgetsRef.current[widgetId] = {}
      })
    },
    onDragGroup: ({ events, inputEvent }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const nextDraftsById = {}
      const lockedTranslate = getAxisLockedTranslate(origin, events[0]?.beforeTranslate || [0, 0], inputEvent || events[0]?.inputEvent)

      events.forEach((childEvent) => {
        const widgetId = getWidgetIdFromTarget(childEvent.target)
        const widget = widgetId ? renderedWidgetMap[widgetId] : null
        const widgetOrigin = widgetId ? origin.widgetsById[widgetId] : null

        if (!widgetId || !widget || !widgetOrigin) {
          return
        }

        const nextDraft = {
          ...draftWidgetsRef.current[widgetId],
          x: widgetOrigin.x + lockedTranslate[0],
          y: widgetOrigin.y + lockedTranslate[1],
        }

        nextDraftsById[widgetId] = nextDraft
        applyLiveWidgetStyles(childEvent.target, widget, nextDraft, globalScale)
      })

      if (Object.keys(nextDraftsById).length) {
        setLiveWidgetDraftsBatch(nextDraftsById)
      }
    },
    onDragGroupEnd: () => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const draggedWidgetIds = origin.widgetIds?.length ? [...origin.widgetIds] : [...groupDragSelectionIds]
      const updatesById = draggedWidgetIds.reduce((accumulator, widgetId) => {
        const draft = draftWidgetsRef.current[widgetId]
        const widgetOrigin = origin.widgetsById[widgetId]

        if (!draft || !widgetOrigin) {
          return accumulator
        }

        accumulator[widgetId] = {
          x: Math.round(draft.x ?? widgetOrigin.x),
          y: Math.round(draft.y ?? widgetOrigin.y),
        }
        return accumulator
      }, {})

      if (Object.keys(updatesById).length) {
        commitWidgetUpdates(updatesById)
      }

      clearWidgetDrafts(draggedWidgetIds)
      setIsGroupDragActive(false)
      setGroupDragSelectionIds([])
      interactionStartRef.current = null
    },
  }
}
