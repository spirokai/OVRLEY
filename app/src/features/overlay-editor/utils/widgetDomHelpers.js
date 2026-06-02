/**
 * Widget DOM helpers — live style application and target element inspection
 * used during drag, resize, and scale interactions.
 */

import { buildWidgetTransform } from '@/lib/geometryUtils'
import { isBoxedMetricWidget } from '@/lib/display-type-behavior'
import { getWidgetSceneOrigin } from './overlayEditorHelpers'

/**
 * Removes a single widget's draft from the mutable ref.
 *
 * @param {React.MutableRefObject<Object<string, Object>>} draftWidgetsRef - Draft ref to mutate.
 * @param {string} widgetId - Widget ID to clear.
 */
export function clearLiveWidgetDraft(draftWidgetsRef, widgetId) {
  delete draftWidgetsRef.current[widgetId]
}

/**
 * Removes multiple widget drafts from the mutable ref.
 *
 * @param {React.MutableRefObject<Object<string, Object>>} draftWidgetsRef - Draft ref to mutate.
 * @param {string[]} widgetIds - Widget IDs to clear.
 */
export function clearLiveWidgetDrafts(draftWidgetsRef, widgetIds) {
  widgetIds.forEach((widgetId) => {
    delete draftWidgetsRef.current[widgetId]
  })
}

/**
 * Extracts the widget ID from a DOM element's data-widget-id attribute.
 *
 * @param {EventTarget} target - DOM element to inspect.
 * @returns {string|null} Widget ID or null if not found.
 */
export function getWidgetIdFromTarget(target) {
  if (!(target instanceof HTMLElement)) {
    return null
  }

  return target.dataset.widgetId || null
}

function parseWidgetBoundsValue(value) {
  const numericValue = Number(value)
  return Number.isFinite(numericValue) ? numericValue : null
}

/**
 * Reads the pre-computed visual bounds from a widget element's
 * data-widget-bounds-* attributes. These are set during rendering
 * by OverlayCanvasWidget based on metricPreviewModel.
 *
 * @param {HTMLElement} target - Widget DOM element.
 * @returns {{ minX: number, minY: number, maxX: number, maxY: number, width: number, height: number }|null}
 *   Visual bounds or null if attributes are missing/invalid.
 */
export function getWidgetVisualBoundsFromTarget(target) {
  if (!(target instanceof HTMLElement)) {
    return null
  }

  const minX = parseWidgetBoundsValue(target.dataset.widgetBoundsLeft)
  const minY = parseWidgetBoundsValue(target.dataset.widgetBoundsTop)
  const maxX = parseWidgetBoundsValue(target.dataset.widgetBoundsRight)
  const maxY = parseWidgetBoundsValue(target.dataset.widgetBoundsBottom)

  if (minX === null || minY === null || maxX === null || maxY === null) {
    return null
  }

  return {
    minX,
    minY,
    maxX,
    maxY,
    width: Math.max(maxX - minX, 0),
    height: Math.max(maxY - minY, 0),
  }
}

/**
 * Applies live widget position and dimension styles directly to the DOM
 * during drag/resize/rotate interactions — bypasses React re-render for
 * responsive feedback.
 *
 * @param {HTMLElement|null} target - Widget DOM element.
 * @param {object} widget - Widget definition (used for fallback values).
 * @param {object} draft - Live draft with x, y, width, height, rotation.
 * @param {number} globalScale - Global scale factor.
 */
export function applyLiveWidgetStyles(target, widget, draft, globalScale) {
  if (!target || !widget) {
    return
  }

  const visualBounds = getWidgetVisualBoundsFromTarget(target)
  const origin = getWidgetSceneOrigin(widget, draft, visualBounds, {
    boundsScale: isBoxedMetricWidget(widget) ? 1 : globalScale,
  })
  const nextWidth = draft.width ?? widget.data.width
  const nextHeight = draft.height ?? widget.data.height
  const nextRotation = draft.rotation ?? (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const isBoxed = isBoxedMetricWidget(widget)
  const renderScale = isBoxed ? globalScale || 1 : 1
  const nextScale = (draft.scale ?? 1) * (isBoxed ? 1 : globalScale)

  target.style.left = `${origin.x}px`
  target.style.top = `${origin.y}px`

  if (typeof nextWidth === 'number') {
    target.style.width = `${nextWidth * renderScale}px`
  }

  if (typeof nextHeight === 'number') {
    target.style.height = `${nextHeight * renderScale}px`
  }

  target.style.transform =
    buildWidgetTransform({
      rotation: nextRotation,
      scale: nextScale,
    }) || ''
}

/**
 * Applies live position styles after a scale interaction — updates left/top
 * and transform on the DOM element directly.
 *
 * @param {HTMLElement|null} target - Widget DOM element.
 * @param {object} widget - Widget definition.
 * @param {object} draft - Live draft with position overrides.
 * @param {number} globalScale - Global scale factor.
 */
export function applyLiveScalePositionStyles(target, widget, draft, globalScale, visualBoundsOverride = null) {
  if (!target || !widget) {
    return
  }

  const nextRotation = draft.rotation ?? (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const transforms = []

  if (draft.scale_factor !== undefined) {
    const tx = draft.translate_x ?? 0
    const ty = draft.translate_y ?? 0
    transforms.push(`translate(${tx}px, ${ty}px)`)
    if (nextRotation) {
      transforms.push(`rotate(${nextRotation}deg)`)
    }
    transforms.push(`scale(${globalScale * draft.scale_factor})`)
    target.style.left = `${draft.scale_start_left}px`
    target.style.top = `${draft.scale_start_top}px`
    target.style.transform = transforms.join(' ')
    return
  }

  const visualBounds = visualBoundsOverride ?? getWidgetVisualBoundsFromTarget(target)
  const draftOrigin = getWidgetSceneOrigin(widget, draft, visualBounds, {
    boundsScale: isBoxedMetricWidget(widget) ? 1 : globalScale,
  })

  if (globalScale !== 1) {
    transforms.push(`scale(${globalScale})`)
  }

  target.style.left = `${draftOrigin.x}px`
  target.style.top = `${draftOrigin.y}px`
  target.style.transform = transforms.join(' ')
}
