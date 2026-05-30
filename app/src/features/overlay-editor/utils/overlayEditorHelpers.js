/**
 * Editor overlay helpers — selection, intersection, scene origin, and
 * scaled data computation. Pure functions, no DOM manipulation.
 */

import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from '../data/overlayEditorConstants'
import { clamp } from '@/lib/geometryUtils'

/**
 * Checks whether the target element is inside an editable input, textarea,
 * select, or contenteditable element.
 *
 * @param {EventTarget} target - DOM event target to inspect.
 * @returns {boolean} True if target is inside an editable element.
 */
export function isEditableElement(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(target.closest('input, textarea, select, [contenteditable="true"]'))
}

/**
 * Checks whether a mouse/pointer event has a multi-selection modifier
 * key held (meta, ctrl, or shift).
 *
 * @param {MouseEvent|PointerEvent} event - DOM pointer event.
 * @returns {boolean} True if metaKey, ctrlKey, or shiftKey is pressed.
 */
export function hasSelectionModifier(event) {
  return event.metaKey || event.ctrlKey || event.shiftKey
}

/**
 * Builds a normalized selection rectangle from two arbitrary points,
 * computing min/max in each dimension.
 *
 * @param {{ x: number, y: number }} start - Start point in scene coordinates.
 * @param {{ x: number, y: number }} end - End point in scene coordinates.
 * @returns {{ x: number, y: number, width: number, height: number }} Normalized rect.
 */
export function buildSelectionRect(start, end) {
  const left = Math.min(start.x, end.x)
  const top = Math.min(start.y, end.y)
  const right = Math.max(start.x, end.x)
  const bottom = Math.max(start.y, end.y)

  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  }
}

/**
 * Checks whether two axis-aligned rectangles intersect.
 *
 * @param {{ x: number, y: number, width: number, height: number }} firstRect - First rectangle.
 * @param {{ x: number, y: number, width: number, height: number }} secondRect - Second rectangle.
 * @returns {boolean} True if rectangles overlap.
 */
export function rectanglesIntersect(firstRect, secondRect) {
  return !(
    firstRect.x + firstRect.width < secondRect.x ||
    secondRect.x + secondRect.width < firstRect.x ||
    firstRect.y + firstRect.height < secondRect.y ||
    secondRect.y + secondRect.height < firstRect.y
  )
}

/**
 * Filters and orders the selection set to match the canonical widget order,
 * removing any IDs that no longer exist in the widget list.
 *
 * @param {string[]} widgetIds - Unordered selected widget IDs.
 * @param {string[]} orderedWidgetIds - Reference-order list of all widget IDs.
 * @returns {string[]} Filtered IDs in canonical order.
 */
export function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set(widgetIds.filter(Boolean))
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

/**
 * Returns the primary selection ID from a list — prefers the given preferredId
 * if it exists in the list, otherwise returns the last widget ID.
 *
 * @param {string[]} widgetIds - Current selection list.
 * @param {string|null} [preferredId=null] - Preferred primary ID.
 * @returns {string|null} Primary widget ID or null if selection is empty.
 */
export function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}

/**
 * Computes the scene-space origin (left, top) for positioning a widget's
 * DOM element. Accounts for visual bounds offsets and gradient value
 * offset adjustment.
 *
 * @param {object} widget - Widget definition.
 * @param {object|null} [draft=null] - Optional live widget draft overrides.
 * @param {object|null} [visualBounds=null] - Pre-computed tight visual bounds ({ minX, minY }).
 * @param {object} [options={}]
 * @param {number} [options.boundsScale=1] - Scale factor for visual bounds offset.
 * @returns {{ x: number, y: number }} Widget origin in scene-space pixels.
 */
export function getWidgetSceneOrigin(widget, draft = null, visualBounds = null, { boundsScale = 1 } = {}) {
  const data = draft ? { ...widget.data, ...draft } : widget.data
  const x = data.x ?? 0
  const gradientYOffset = widget.type === 'gradient' ? Math.min(0, -(data.value_offset ?? 0)) : 0
  const boundsOffsetX = (visualBounds?.minX ?? 0) * boundsScale
  const boundsOffsetY = (visualBounds?.minY ?? 0) * boundsScale

  return {
    x: x + boundsOffsetX,
    y: (data.y ?? 0) + gradientYOffset + boundsOffsetY,
  }
}

/**
 * Builds a widget data draft from a scaling interaction — computes new
 * font_size, icon_size, icon offsets, triangle_width, and value_offset
 * based on the uniform scale factor.
 *
 * Only includes fields relevant to the widget's category and type.
 *
 * @param {object} origin - Interaction start snapshot ({ fontSize, iconSize, iconOffsetX, iconOffsetY, triangleWidth, valueOffset }).
 * @param {number} scaleFactor - Uniform scale multiplier.
 * @param {object} widget - Widget definition (used to determine category).
 * @returns {object} Draft with scaled properties.
 */
export function buildScaledWidgetDataDraft(origin, scaleFactor, widget, { round = true } = {}) {
  const r = round ? Math.round : (v) => v
  const nextFontSize = clamp(r((origin.fontSize || 60) * scaleFactor), 8, 400)
  const nextIconSize = clamp(r((origin.iconSize || 28) * scaleFactor), 0, 400)
  const nextIconOffsetX = r((origin.iconOffsetX || 0) * scaleFactor)
  const nextIconOffsetY = r((origin.iconOffsetY || 0) * scaleFactor)
  const nextTriangleWidth = clamp(r((origin.triangleWidth ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH) * scaleFactor), 0, 600)
  const nextValueOffset = r((origin.valueOffset || 0) * scaleFactor)

  const nextDraft = {
    font_size: nextFontSize,
  }

  if (widget?.category === 'values' && widget.type !== 'gradient') {
    Object.assign(nextDraft, {
      icon_size: nextIconSize,
      icon_offset_x: nextIconOffsetX,
      icon_offset_y: nextIconOffsetY,
    })
  }

  if (widget?.type === 'gradient') {
    Object.assign(nextDraft, {
      triangle_width: nextTriangleWidth,
      value_offset: nextValueOffset,
    })
  }

  return nextDraft
}
