/**
 * Drag handler group for OverlayMoveable — single-drag and group-drag logic.
 */

import { AXIS_LOCK_THRESHOLD } from '../data/overlayEditorConstants'
import { applyLiveWidgetStyles, getWidgetIdFromTarget } from '../utils/widgetDomHelpers'
import { buildDragInteractionLayout, captureWidgetLayout, getWidgetInteractionPosition } from '../utils/widgetInteractionGeometry'

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
 * @param {Array} ctx.effectiveSelectedWidgetIds
 * @param {Function} ctx.setLiveWidgetDraft
 * @param {Function} ctx.setLiveWidgetDraftsBatch
 * @param {Function} ctx.commitWidgetUpdate
 * @param {Function} ctx.commitWidgetUpdates
 * @param {Function} ctx.clearWidgetDraft
 * @param {Function} ctx.clearWidgetDrafts
 * @param {Function} ctx.setIsGroupDragActive
 * @param {Function} ctx.setGroupDragSelectionIds
 * @param {Function} ctx.beginWidgetInteraction
 * @param {Function} ctx.endWidgetInteraction
 * @returns {object} Drag handler methods.
 */
export function useDragHandlers({
  interactionStartRef,
  draftWidgetsRef,
  selectedWidget,
  selectedWidgets,
  globalScale,
  effectiveSelectedWidgetIds,
  setLiveWidgetDraft,
  setLiveWidgetDraftsBatch,
  commitWidgetUpdate,
  commitWidgetUpdates,
  clearWidgetDraft,
  clearWidgetDrafts,
  setIsGroupDragActive,
  setGroupDragSelectionIds,
  beginWidgetInteraction,
  endWidgetInteraction,
}) {
  // Drag handlers — single and group drag with axis lock via Ctrl key
  return {
    onDragStart: ({ target }) => {
      if (!selectedWidget || effectiveSelectedWidgetIds.length !== 1) return

      const layout = captureWidgetLayout(target, selectedWidget, globalScale)
      const position = getWidgetInteractionPosition(selectedWidget, layout)
      interactionStartRef.current = {
        id: selectedWidget.id,
        x: position.x,
        y: position.y,
        layout,
        type: 'single-drag',
      }
      beginWidgetInteraction(selectedWidget.id, 'drag')
    },
    onDrag: ({ beforeTranslate, inputEvent, target }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'single-drag') return
      const lockedTranslate = getAxisLockedTranslate(origin, beforeTranslate, inputEvent)

      const nextDraft = {
        ...(draftWidgetsRef.current[origin.id]?.data ?? {}),
        x: origin.x + lockedTranslate[0],
        y: origin.y + lockedTranslate[1],
      }
      const layout = buildDragInteractionLayout(origin.layout, lockedTranslate[0], lockedTranslate[1])

      setLiveWidgetDraft(origin.id, nextDraft, layout)
      applyLiveWidgetStyles(target, layout, globalScale)
    },
    onDragEnd: () => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'single-drag') return

      const draft = draftWidgetsRef.current[origin.id]?.data
      if (draft) {
        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
        })
      }

      clearWidgetDraft(origin.id)
      endWidgetInteraction(origin.id)
      interactionStartRef.current = null
    },
    onDragGroupStart: ({ events = [] } = {}) => {
      if (!selectedWidgets.length) return

      const draggedWidgetIds = [...effectiveSelectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      interactionStartRef.current = {
        type: 'group-drag',
        widgetIds: draggedWidgetIds,
        widgetsById: Object.fromEntries(
          selectedWidgets.map((widget) => {
            const layout = captureWidgetLayout(events.find((event) => getWidgetIdFromTarget(event.target) === widget.id)?.target, widget, globalScale)
            const position = getWidgetInteractionPosition(widget, layout)
            return [widget.id, { x: position.x, y: position.y, layout }]
          }),
        ),
      }

      draggedWidgetIds.forEach((widgetId) => {
        draftWidgetsRef.current[widgetId] = { data: {}, layout: null }
      })
      beginWidgetInteraction(draggedWidgetIds[0], 'group-drag')
    },
    onDragGroup: ({ events, inputEvent }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const nextDraftsById = {}
      const lockedTranslate = getAxisLockedTranslate(origin, events[0]?.beforeTranslate || [0, 0], inputEvent || events[0]?.inputEvent)

      events.forEach((childEvent) => {
        const widgetId = getWidgetIdFromTarget(childEvent.target)
        const widgetOrigin = widgetId ? origin.widgetsById[widgetId] : null

        if (!widgetId || !widgetOrigin) {
          return
        }

        const nextDraft = {
          ...(draftWidgetsRef.current[widgetId]?.data ?? {}),
          x: widgetOrigin.x + lockedTranslate[0],
          y: widgetOrigin.y + lockedTranslate[1],
        }

        const layout = buildDragInteractionLayout(widgetOrigin.layout, lockedTranslate[0], lockedTranslate[1])
        nextDraftsById[widgetId] = { data: nextDraft, layout }
        applyLiveWidgetStyles(childEvent.target, layout, globalScale)
      })

      if (Object.keys(nextDraftsById).length) {
        setLiveWidgetDraftsBatch(nextDraftsById)
      }
    },
    onDragGroupEnd: () => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const updatesById = origin.widgetIds.reduce((accumulator, widgetId) => {
        const draft = draftWidgetsRef.current[widgetId]?.data
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

      clearWidgetDrafts(origin.widgetIds)
      endWidgetInteraction(origin.widgetIds[0])
      setIsGroupDragActive(false)
      setGroupDragSelectionIds([])
      interactionStartRef.current = null
    },
  }
}
