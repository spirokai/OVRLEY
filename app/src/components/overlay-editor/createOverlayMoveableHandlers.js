/**
 * Provides overlay editor helpers for create overlay moveable handlers.
 */

import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import {
  applyLiveScalePositionStyles,
  applyLiveWidgetStyles,
  buildScaledWidgetDataDraft,
  getWidgetIdFromTarget,
} from './overlayEditorHelpers'
import { clamp } from './utils'

/**
 * Provides overlay moveable handlers state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.clearWidgetDraft - Value for clear widget draft.
 * @param {*} options.clearWidgetDrafts - Value for clear widget drafts.
 * @param {*} options.commitWidgetUpdate - Value for commit widget update.
 * @param {*} options.commitWidgetUpdates - Value for commit widget updates.
 * @param {*} options.draftWidgetsRef - Value for draft widgets ref.
 * @param {*} options.effectiveSelectedWidgetIds - Value for effective selected widget ids.
 * @param {*} options.globalScale - Scale factor applied to the overlay preview.
 * @param {*} options.groupDragSelectionIds - Value for group drag selection ids.
 * @param {*} options.interactionStartRef - Value for interaction start ref.
 * @param {*} options.renderedWidgetMap - Value for rendered widget map.
 * @param {*} options.scalePreviewFrameRef - Value for scale preview frame ref.
 * @param {*} options.selectedTarget - Value for selected target.
 * @param {*} options.selectedWidget - Value for selected widget.
 * @param {*} options.selectedWidgets - Value for selected widgets.
 * @param {*} options.setGroupDragSelectionIds - Value for set group drag selection ids.
 * @param {*} options.setIsGroupDragActive - Value for set is group drag active.
 * @param {*} options.setLiveWidgetDraft - Value for set live widget draft.
 * @param {*} options.setLiveWidgetDraftsBatch - Value for set live widget drafts batch.
 * @returns {object} Result produced by the helper.
 */
export default function useOverlayMoveableHandlers({
  clearWidgetDraft,
  clearWidgetDrafts,
  commitWidgetUpdate,
  commitWidgetUpdates,
  draftWidgetsRef,
  effectiveSelectedWidgetIds,
  globalScale,
  groupDragSelectionIds,
  interactionStartRef,
  renderedWidgetMap,
  scalePreviewFrameRef,
  selectedTarget,
  selectedWidget,
  selectedWidgets,
  setGroupDragSelectionIds,
  setIsGroupDragActive,
  setLiveWidgetDraft,
  setLiveWidgetDraftsBatch,
}) {
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
    onDrag: ({ beforeTranslate, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: origin.x + beforeTranslate[0],
        y: origin.y + beforeTranslate[1],
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
    onDragGroup: ({ events }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const nextDraftsById = {}

      events.forEach((childEvent) => {
        const widgetId = getWidgetIdFromTarget(childEvent.target)
        const widget = widgetId ? renderedWidgetMap[widgetId] : null
        const widgetOrigin = widgetId ? origin.widgetsById[widgetId] : null

        if (!widgetId || !widget || !widgetOrigin) {
          return
        }

        const nextDraft = {
          ...draftWidgetsRef.current[widgetId],
          x: widgetOrigin.x + childEvent.beforeTranslate[0],
          y: widgetOrigin.y + childEvent.beforeTranslate[1],
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

      const draggedWidgetIds = origin.widgetIds?.length
        ? [...origin.widgetIds]
        : [...groupDragSelectionIds]
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
    onResizeStart: ({ dragStart }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        width: selectedWidget.data.width ?? 0,
        height: selectedWidget.data.height ?? 0,
        markerSize: selectedWidget.data.marker_size ?? null,
        type: 'resize',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onResize: ({ width, height, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextX = origin.x + drag.beforeTranslate[0]
      const nextY = origin.y + drag.beforeTranslate[1]
      const nextWidth = Math.max(width, 8)
      const nextHeight = Math.max(height, 8)
      const widthScale = origin.width ? nextWidth / origin.width : 1
      const heightScale = origin.height ? nextHeight / origin.height : 1
      const markerScale = (widthScale + heightScale) / 2
      const nextMarkerSize =
        origin.markerSize === null
          ? undefined
          : clamp(Math.round(origin.markerSize * markerScale), 0, 400)

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: nextX,
        y: nextY,
        width: nextWidth,
        height: nextHeight,
        ...(nextMarkerSize === undefined
          ? {}
          : { marker_size: nextMarkerSize }),
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(
        target ?? drag.target,
        selectedWidget,
        nextDraft,
        globalScale,
      )
    },
    onResizeEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          width: Math.max(Math.round(draft.width ?? 0), 0),
          height: Math.max(Math.round(draft.height ?? 0), 0),
          ...(draft.marker_size === undefined
            ? {}
            : { marker_size: Math.max(Math.round(draft.marker_size), 0) }),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onScaleStart: ({ dragStart, target }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        fontSize: selectedWidget.data.font_size ?? 60,
        iconSize: selectedWidget.data.icon_size ?? 28,
        iconOffsetX: selectedWidget.data.icon_offset_x ?? 0,
        iconOffsetY: selectedWidget.data.icon_offset_y ?? 0,
        triangleWidth:
          selectedWidget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
        valueOffset: selectedWidget.data.value_offset ?? 0,
        renderedWidth: target?.offsetWidth ?? selectedTarget?.offsetWidth ?? 0,
        renderedHeight:
          target?.offsetHeight ?? selectedTarget?.offsetHeight ?? 0,
        type: 'scale',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onScale: ({ scale, direction, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return
      const uniformScale = Number.isFinite(scale?.[0])
        ? scale[0]
        : Number.isFinite(scale?.[1])
          ? scale[1]
          : 1

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        scale_direction: direction,
        ...buildScaledWidgetDataDraft(origin, uniformScale),
      }

      draftWidgetsRef.current[origin.id] = nextDraft
      setLiveWidgetDraft(
        origin.id,
        buildScaledWidgetDataDraft(origin, uniformScale),
      )

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
      }

      scalePreviewFrameRef.current = requestAnimationFrame(() => {
        const targetNode = target ?? selectedTarget
        if (!targetNode) return

        const measuredWidth = targetNode.offsetWidth
        const measuredHeight = targetNode.offsetHeight
        const measuredDraft = {
          ...draftWidgetsRef.current[origin.id],
          x:
            origin.x +
            (direction?.[0] === -1 ? origin.renderedWidth - measuredWidth : 0),
          y:
            origin.y +
            (direction?.[1] === -1
              ? origin.renderedHeight - measuredHeight
              : 0),
        }

        draftWidgetsRef.current[origin.id] = measuredDraft
        applyLiveScalePositionStyles(
          targetNode,
          selectedWidget,
          measuredDraft,
          globalScale,
        )
      })
    },
    onScaleEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
        scalePreviewFrameRef.current = null
      }

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        const targetNode = selectedTarget
        const measuredWidth =
          targetNode?.offsetWidth ?? origin.renderedWidth ?? 0
        const measuredHeight =
          targetNode?.offsetHeight ?? origin.renderedHeight ?? 0
        const finalDirection = Array.isArray(draft.scale_direction)
          ? draft.scale_direction
          : [1, 1]
        const finalX =
          origin.x +
          (finalDirection[0] === -1 ? origin.renderedWidth - measuredWidth : 0)
        const finalY =
          origin.y +
          (finalDirection[1] === -1
            ? origin.renderedHeight - measuredHeight
            : 0)

        commitWidgetUpdate(origin.id, {
          x: Math.round(finalX),
          y: Math.round(finalY),
          font_size: draft.font_size ?? origin.fontSize ?? 60,
          icon_size: draft.icon_size ?? origin.iconSize ?? 28,
          icon_offset_x: draft.icon_offset_x ?? origin.iconOffsetX ?? 0,
          icon_offset_y: draft.icon_offset_y ?? origin.iconOffsetY ?? 0,
          triangle_width: draft.triangle_width ?? origin.triangleWidth ?? 0,
          value_offset: draft.value_offset ?? origin.valueOffset ?? 0,
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onRotateStart: () => {
      if (!selectedWidget) return

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        rotation: selectedWidget.data.rotation ?? 0,
        type: 'rotate',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onRotate: ({ beforeRotate, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextX = origin.x + (drag?.beforeTranslate?.[0] ?? 0)
      const nextY = origin.y + (drag?.beforeTranslate?.[1] ?? 0)
      const nextRotation = beforeRotate
      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: nextX,
        y: nextY,
        rotation: nextRotation,
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(target, selectedWidget, nextDraft, globalScale)
    },
    onRotateEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        const normalizedRotation =
          (((draft.rotation ?? origin.rotation ?? 0) % 360) + 360) % 360

        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          rotation: Number(normalizedRotation.toFixed(1)),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }
}
