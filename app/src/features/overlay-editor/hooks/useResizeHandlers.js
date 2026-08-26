/**
 * Resize handler group for OverlayMoveable.
 */

import { updateLiveWidgetDraft } from '../utils/widgetDomHelpers'
import { buildFrameInteractionLayout, captureWidgetLayout, getWidgetInteractionPosition } from '../utils/widgetInteractionGeometry'
import { buildLiveResizeUpdate, buildResizeUpdate, captureResizeOrigin } from '../utils/widgetResizeScaling'
import { isFramedWidget } from '@/lib/widget/display-type-behavior'

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
 * @param {Function} ctx.beginWidgetInteraction
 * @param {Function} ctx.endWidgetInteraction
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
  beginWidgetInteraction,
  endWidgetInteraction,
}) {
  // Resize handlers — captures origin dimensions, computes scaled size, commits on end
  return {
    onResizeStart: ({ dragStart, target }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      const frameData = selectedWidget.data
      const resizeOrigin = captureResizeOrigin(selectedWidget, frameData)
      const layout = captureWidgetLayout(target ?? dragStart?.target, selectedWidget, globalScale)
      const position = getWidgetInteractionPosition(selectedWidget, layout)
      interactionStartRef.current = {
        id: selectedWidget.id,
        x: position.x,
        y: position.y,
        type: 'resize',
        layout,
        ...resizeOrigin,
      }
      beginWidgetInteraction(selectedWidget.id, 'resize')
    },
    onResize: ({ width, height, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextX = origin.x + drag.beforeTranslate[0]
      const nextY = origin.y + drag.beforeTranslate[1]
      const dimensionScale = isFramedWidget(selectedWidget) ? Math.max(Number(globalScale) || 1, 0.1) : 1
      const nextWidth = Math.max(width / dimensionScale, 8)
      const nextHeight = Math.max(height / dimensionScale, 8)
      const liveResizeUpdate = buildLiveResizeUpdate(origin, { x: nextX, y: nextY, width: nextWidth, height: nextHeight }, selectedWidget)
      const liveLayout = buildFrameInteractionLayout(origin.layout, {
        width: nextWidth * (globalScale || 1),
        height: nextHeight * (globalScale || 1),
        translateX: drag.beforeTranslate[0],
        translateY: drag.beforeTranslate[1],
      })

      updateLiveWidgetDraft({
        draftWidgetsRef,
        setLiveWidgetDraft,
        widgetId: origin.id,
        widget: selectedWidget,
        updates: liveResizeUpdate,
        target: isFramedWidget(selectedWidget) ? (target ?? drag.target) : null,
        globalScale,
        layout: liveLayout,
      })
    },
    onResizeEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]?.data
      if (draft) {
        const geometryPatch = {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          width: Math.max(Math.round(draft.width ?? 0), 0),
          height: Math.max(Math.round(draft.height ?? 0), 0),
        }
        commitWidgetUpdate(origin.id, buildResizeUpdate(origin, geometryPatch, { round: true }))
      }

      clearWidgetDraft(origin.id)
      endWidgetInteraction(origin.id)
      interactionStartRef.current = null
    },
  }
}
