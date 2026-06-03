/**
 * Scale handler group for OverlayMoveable.
 */

import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from '../data/overlayEditorConstants'
import { applyLiveScalePositionStyles, getWidgetVisualBoundsFromTarget } from '../utils/widgetDomHelpers'
import { buildScaledWidgetDataDraft } from '../utils/overlayEditorHelpers'
import { flushSync } from 'react-dom'

/**
 * Creates scale-related moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef
 * @param {object} ctx.draftWidgetsRef
 * @param {object} ctx.scalePreviewFrameRef
 * @param {object} ctx.selectedWidget
 * @param {object} ctx.selectedTarget
 * @param {number} ctx.globalScale
 * @param {Function} ctx.setLiveWidgetPreview
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
  globalScale,
  setLiveWidgetPreview,
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
      const startTarget = target ?? selectedTarget
      const renderedLeft = startTarget ? parseFloat(startTarget.style.left) || 0 : 0
      const renderedTop = startTarget ? parseFloat(startTarget.style.top) || 0 : 0

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
        renderedWidth: startTarget?.offsetWidth ?? 0,
        renderedHeight: startTarget?.offsetHeight ?? 0,
        renderedMinX: currentBounds?.minX ?? 0,
        renderedMinY: currentBounds?.minY ?? 0,
        renderedMaxX: currentBounds?.maxX ?? 0,
        renderedMaxY: currentBounds?.maxY ?? 0,
        renderedLeft,
        renderedTop,
        type: 'scale',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onScale: ({ scale, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return
      const rawScale = Number.isFinite(scale?.[0]) ? scale[0] : Number.isFinite(scale?.[1]) ? scale[1] : 1
      const safeGlobalScale = globalScale > 0 ? globalScale : 1
      const uniformScale = rawScale / safeGlobalScale

      const tx = drag?.beforeTranslate?.[0] ?? 0
      const ty = drag?.beforeTranslate?.[1] ?? 0

      const gradientYOffset = selectedWidget.type === 'gradient' ? Math.min(0, -origin.valueOffset) : 0
      const nextX = origin.x + tx + origin.renderedMinX * (1 - uniformScale) * globalScale
      const nextY = origin.y + ty + (origin.renderedMinY * globalScale + gradientYOffset) * (1 - uniformScale)

      const preview = {
        left: origin.renderedLeft,
        top: origin.renderedTop,
        width: origin.renderedWidth,
        height: origin.renderedHeight,
        scaleFactor: uniformScale,
        translateX: tx,
        translateY: ty,
      }
      const nextDraft = {
        scaleFactor: uniformScale,
        translateX: tx,
        translateY: ty,
        x: nextX,
        y: nextY,
      }

      draftWidgetsRef.current[origin.id] = nextDraft
      flushSync(() => {
        setLiveWidgetPreview(origin.id, preview)
      })

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
      }

      scalePreviewFrameRef.current = requestAnimationFrame(() => {
        const targetNode = target ?? selectedTarget
        if (!targetNode) return
        applyLiveScalePositionStyles(targetNode, selectedWidget, preview, globalScale)
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
        const finalScale = draft.scaleFactor ?? 1
        const scaledDraft = buildScaledWidgetDataDraft(origin, finalScale, selectedWidget, { round: true })

        const tx = draft.translateX ?? 0
        const ty = draft.translateY ?? 0
        const gradientYOffset = selectedWidget.type === 'gradient' ? Math.min(0, -origin.valueOffset) : 0
        const finalX = origin.x + tx + origin.renderedMinX * (1 - finalScale) * globalScale
        const finalY = origin.y + ty + (origin.renderedMinY * globalScale + gradientYOffset) * (1 - finalScale)

        commitWidgetUpdate(origin.id, {
          x: Math.round(finalX),
          y: Math.round(finalY),
          font_size: scaledDraft.font_size,
          ...(selectedWidget?.category === 'values' && selectedWidget.type !== 'gradient'
            ? {
                icon_size: scaledDraft.icon_size,
                icon_offset_x: scaledDraft.icon_offset_x,
                icon_offset_y: scaledDraft.icon_offset_y,
              }
            : {}),
          ...(selectedWidget?.type === 'gradient'
            ? {
                triangle_width: scaledDraft.triangle_width,
                value_offset: scaledDraft.value_offset,
              }
            : {}),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }
}
