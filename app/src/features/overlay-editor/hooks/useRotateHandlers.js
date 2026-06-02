/**
 * Rotate handler group for OverlayMoveable.
 */

import { applyLiveWidgetStyles } from '../utils/widgetDomHelpers'
import { buildFrameGeometryUpdate } from '@/lib/metric-widget-resolver'

/**
 * Creates rotate-related moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef
 * @param {object} ctx.draftWidgetsRef
 * @param {object} ctx.selectedWidget
 * @param {number} ctx.globalScale
 * @param {Function} ctx.setLiveWidgetDraft
 * @param {Function} ctx.commitWidgetUpdate
 * @param {Function} ctx.clearWidgetDraft
 * @returns {object} Rotate handler methods.
 */
export function useRotateHandlers({
  interactionStartRef,
  draftWidgetsRef,
  selectedWidget,
  globalScale,
  setLiveWidgetDraft,
  commitWidgetUpdate,
  clearWidgetDraft,
}) {
  // Rotate handlers — captures origin rotation, applies rotation + position offset, normalizes on end
  return {
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
        const normalizedRotation = (((draft.rotation ?? origin.rotation ?? 0) % 360) + 360) % 360
        const geometryPatch = {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          rotation: Number(normalizedRotation.toFixed(1)),
        }
        commitWidgetUpdate(origin.id, buildFrameGeometryUpdate(selectedWidget?.data, geometryPatch))
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }
}
