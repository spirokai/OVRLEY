/**
 * Widget DOM helpers — live style application and target element inspection
 * used during drag, resize, and scale interactions.
 */

import { isFramedWidget } from '@/lib/widget/display-type-behavior'
import { getLiveWidgetTransform } from './widgetInteractionGeometry'

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
 * Applies a captured live layout directly to the DOM. The layout is calculated
 * from the interaction origin, so changing visual bounds cannot move the
 * widget's anchor during the interaction.
 *
 * @param {HTMLElement|null} target - Widget DOM element.
 * @param {object|null} layout - Live interaction layout.
 * @param {number} globalScale - Global scale factor.
 */
export function applyLiveWidgetStyles(target, layout, globalScale) {
  if (!target || !layout) {
    return
  }

  target.style.left = `${layout.left}px`
  target.style.top = `${layout.top}px`
  target.style.width = `${layout.width}px`
  target.style.height = `${layout.height}px`
  target.style.transform = getLiveWidgetTransform(layout, globalScale)
  target.style.transformOrigin = layout.mode === 'scale' ? `${layout.transformOriginX}px 0px` : 'top left'

  if (layout.mode === 'frame' && target.firstElementChild) {
    target.firstElementChild.style.width = '100%'
    target.firstElementChild.style.height = '100%'
  }
}

/**
 * Applies only data-driven dimension styles without deriving position from
 * current visual bounds. This is used by sidebar size controls.
 *
 * @param {HTMLElement|null} target - Widget DOM element.
 * @param {object} widget - Widget definition.
 * @param {object} draft - Live data draft.
 * @param {number} globalScale - Global scale factor.
 */
export function applyLiveWidgetDataStyles(target, widget, draft, globalScale) {
  if (!target || !widget) return

  const isFramed = isFramedWidget(widget)
  const renderScale = isFramed ? globalScale || 1 : 1
  const nextWidth = draft.width ?? widget.data.width
  const nextHeight = draft.height ?? widget.data.height

  if (typeof nextWidth === 'number') target.style.width = `${nextWidth * renderScale}px`
  if (typeof nextHeight === 'number') target.style.height = `${nextHeight * renderScale}px`
}

/**
 * Stores a live widget data update and applies its geometry immediately.
 * Interaction handlers and sidebar size controls use the same path so config
 * remains untouched until the interaction commits.
 *
 * @param {object} options - Live update options.
 * @param {React.MutableRefObject<Object<string, Object>>} options.draftWidgetsRef - Draft ref to mutate.
 * @param {Function} options.setLiveWidgetDraft - Draft state updater.
 * @param {string} options.widgetId - Widget ID being updated.
 * @param {object} options.updates - Partial widget data update.
 * @param {HTMLElement|null} options.target - Widget DOM element.
 * @param {number} options.globalScale - Global scene scale.
 * @param {object|null} [options.layout] - Captured live interaction layout.
 * @returns {object} Merged live draft.
 */
export function updateLiveWidgetDraft({ draftWidgetsRef, setLiveWidgetDraft, widgetId, widget, updates, target, globalScale, layout = undefined }) {
  const currentDraft = draftWidgetsRef.current[widgetId]?.data ?? {}
  const nextDraft = {
    ...currentDraft,
    ...updates,
  }

  setLiveWidgetDraft(widgetId, nextDraft, layout)
  if (layout) applyLiveWidgetStyles(target, layout, globalScale)
  else applyLiveWidgetDataStyles(target, widget, nextDraft, globalScale)
  return nextDraft
}
