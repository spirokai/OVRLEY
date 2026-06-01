/**
 * Resize handler group for OverlayMoveable.
 */

import { applyLiveWidgetStyles } from '../utils/widgetDomHelpers'
import { clamp } from '@/lib/utils'
import { isPlotLikeWidget } from '@/lib/widget-behavior'

/**
 * Creates resize-related moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef
 * @param {object} ctx.draftWidgetsRef
 * @param {object} ctx.selectedWidget
 * @param {number} ctx.globalScale
 * @param {Function} ctx.setLiveWidgetDraft
 * @param {Function} ctx.commitWidgetUpdate
 * @param {Function} ctx.clearWidgetDraft
 * @returns {object} Resize handler methods.
 */
export function useResizeHandlers({
  interactionStartRef,
  draftWidgetsRef,
  selectedWidget,
  globalScale,
  setLiveWidgetDraft,
  commitWidgetUpdate,
  clearWidgetDraft,
}) {
  // Resize handlers — captures origin dimensions, computes scaled size, commits on end
  return {
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
      const dimensionScale = isPlotLikeWidget(selectedWidget) ? Math.max(Number(globalScale) || 1, 0.1) : 1
      const nextWidth = Math.max(width / dimensionScale, 8)
      const nextHeight = Math.max(height / dimensionScale, 8)
      const widthScale = origin.width ? nextWidth / origin.width : 1
      const heightScale = origin.height ? nextHeight / origin.height : 1
      const markerScale = (widthScale + heightScale) / 2
      const nextMarkerSize = origin.markerSize === null ? undefined : clamp(Math.round(origin.markerSize * markerScale), 0, 400)

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: nextX,
        y: nextY,
        width: nextWidth,
        height: nextHeight,
        ...(nextMarkerSize === undefined ? {} : { marker_size: nextMarkerSize }),
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      if (isPlotLikeWidget(selectedWidget)) {
        applyLiveWidgetStyles(target ?? drag.target, selectedWidget, nextDraft, globalScale)
      }
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
          ...(draft.marker_size === undefined ? {} : { marker_size: Math.max(Math.round(draft.marker_size), 0) }),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }
}
