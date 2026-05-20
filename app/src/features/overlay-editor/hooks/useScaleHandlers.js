/**
 * Scale handler group for OverlayMoveable.
 */

import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from '../data/overlayEditorConstants'
import { applyLiveScalePositionStyles, getWidgetVisualBoundsFromTarget } from '../utils/widgetDomHelpers'
import { buildScaledWidgetDataDraft } from '../utils/overlayEditorHelpers'
import { buildMetricWidgetPreviewModel, buildTextWidgetPreviewModel } from '@/features/widget-preview'

function buildScaledVisualBounds(widget, scaledDraft, activity, previewSecond) {
  if (!widget) {
    return null
  }

  const draftWidget = {
    ...widget,
    data: {
      ...widget.data,
      ...scaledDraft,
    },
  }

  return (
    buildMetricWidgetPreviewModel({
      widget: draftWidget,
      activity,
      previewSecond,
    })?.visualBounds ??
    buildTextWidgetPreviewModel({
      widget: draftWidget,
    })?.visualBounds ??
    null
  )
}

/**
 * Creates scale-related moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef
 * @param {object} ctx.draftWidgetsRef
 * @param {object} ctx.scalePreviewFrameRef
 * @param {object} ctx.selectedWidget
 * @param {object} ctx.selectedTarget
 * @param {object|null} ctx.activity
 * @param {number} ctx.previewSecond
 * @param {number} ctx.globalScale
 * @param {Function} ctx.setLiveWidgetDraft
 * @param {Function} ctx.commitWidgetUpdate
 * @param {Function} ctx.clearWidgetDraft
 * @returns {object} Scale handler methods.
 */
export function useScaleHandlers({
  interactionStartRef,
  draftWidgetsRef,
  scalePreviewFrameRef,
  selectedWidget,
  selectedTarget,
  activity,
  previewSecond,
  globalScale,
  setLiveWidgetDraft,
  commitWidgetUpdate,
  clearWidgetDraft,
}) {
  // Scale handlers — uniform scaling of metric widget properties (font, icon, triangle), uses rAF for final position
  return {
    onScaleStart: ({ dragStart, target }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      const currentBounds = getWidgetVisualBoundsFromTarget(target ?? selectedTarget)

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        fontSize: selectedWidget.data.font_size ?? 60,
        iconSize: selectedWidget.data.icon_size ?? 28,
        iconOffsetX: selectedWidget.data.icon_offset_x ?? 0,
        iconOffsetY: selectedWidget.data.icon_offset_y ?? 0,
        triangleWidth: selectedWidget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
        valueOffset: selectedWidget.data.value_offset ?? 0,
        renderedWidth: target?.offsetWidth ?? selectedTarget?.offsetWidth ?? 0,
        renderedHeight: target?.offsetHeight ?? selectedTarget?.offsetHeight ?? 0,
        renderedMaxX: currentBounds?.maxX ?? 0,
        renderedMaxY: currentBounds?.maxY ?? 0,
        type: 'scale',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onScale: ({ scale, direction, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return
      const rawScale = Number.isFinite(scale?.[0]) ? scale[0] : Number.isFinite(scale?.[1]) ? scale[1] : 1
      const safeGlobalScale = globalScale > 0 ? globalScale : 1
      const uniformScale = rawScale / safeGlobalScale

      const scaledDraft = buildScaledWidgetDataDraft(origin, uniformScale, selectedWidget)
      const nextBounds = buildScaledVisualBounds(selectedWidget, scaledDraft, activity, previewSecond)
      const positionedDraft = {
        ...draftWidgetsRef.current[origin.id],
        ...scaledDraft,
        x: origin.x + (direction?.[0] === -1 ? origin.renderedMaxX - (nextBounds?.maxX ?? origin.renderedWidth ?? 0) : 0),
        y: origin.y + (direction?.[1] === -1 ? origin.renderedMaxY - (nextBounds?.maxY ?? origin.renderedHeight ?? 0) : 0),
      }
      const nextDraft = {
        ...positionedDraft,
        scale_direction: direction,
      }

      draftWidgetsRef.current[origin.id] = nextDraft
      setLiveWidgetDraft(origin.id, positionedDraft)

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
      }

      scalePreviewFrameRef.current = requestAnimationFrame(() => {
        const targetNode = target ?? selectedTarget
        if (!targetNode) return
        applyLiveScalePositionStyles(targetNode, selectedWidget, positionedDraft, globalScale, nextBounds)
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
        const finalDirection = Array.isArray(draft.scale_direction) ? draft.scale_direction : [1, 1]
        const finalBounds = buildScaledVisualBounds(selectedWidget, draft, activity, previewSecond)
        const measuredBounds = targetNode ? getWidgetVisualBoundsFromTarget(targetNode) : null
        const resolvedBounds = finalBounds ?? measuredBounds
        const fallbackX = origin.x + (finalDirection[0] === -1 ? origin.renderedMaxX - (resolvedBounds?.maxX ?? origin.renderedWidth ?? 0) : 0)
        const fallbackY = origin.y + (finalDirection[1] === -1 ? origin.renderedMaxY - (resolvedBounds?.maxY ?? origin.renderedHeight ?? 0) : 0)
        const finalX = draft.x ?? fallbackX
        const finalY = draft.y ?? fallbackY

        commitWidgetUpdate(origin.id, {
          x: Math.round(finalX),
          y: Math.round(finalY),
          font_size: draft.font_size ?? origin.fontSize ?? 60,
          ...(selectedWidget?.category === 'values' && selectedWidget.type !== 'gradient'
            ? {
                icon_size: draft.icon_size ?? origin.iconSize ?? 28,
                icon_offset_x: draft.icon_offset_x ?? origin.iconOffsetX ?? 0,
                icon_offset_y: draft.icon_offset_y ?? origin.iconOffsetY ?? 0,
              }
            : {}),
          ...(selectedWidget?.type === 'gradient'
            ? {
                triangle_width: draft.triangle_width ?? origin.triangleWidth ?? 0,
                value_offset: draft.value_offset ?? origin.valueOffset ?? 0,
              }
            : {}),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }
}
