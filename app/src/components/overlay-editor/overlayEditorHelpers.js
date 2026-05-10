/**
 * Provides overlay editor helpers for overlay editor helpers.
 */

import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import { buildWidgetTransform, clamp } from './utils'

/**
 * Clears live widget draft.
 *
 * @param {*} draftWidgetsRef - Value for draft widgets ref.
 * @param {*} widgetId - Identifier of the target widget.
 * @returns {*} Result produced by the helper.
 */
export function clearLiveWidgetDraft(draftWidgetsRef, widgetId) {
  delete draftWidgetsRef.current[widgetId]
}

/**
 * Clears live widget drafts.
 *
 * @param {*} draftWidgetsRef - Value for draft widgets ref.
 * @param {*} widgetIds - Value for widget ids.
 * @returns {*} Result produced by the helper.
 */
export function clearLiveWidgetDrafts(draftWidgetsRef, widgetIds) {
  widgetIds.forEach((widgetId) => {
    delete draftWidgetsRef.current[widgetId]
  })
}

/**
 * Checks whether is editable element.
 *
 * @param {*} target - Target object, element, or value being updated.
 * @returns {boolean} Whether the condition is satisfied.
 */
export function isEditableElement(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(
    target.closest('input, textarea, select, [contenteditable="true"]'),
  )
}

/**
 * Checks whether has selection modifier.
 *
 * @param {*} event - DOM or pointer event for the interaction.
 * @returns {boolean} Whether the condition is satisfied.
 */
export function hasSelectionModifier(event) {
  return event.metaKey || event.ctrlKey || event.shiftKey
}

/**
 * Builds selection rect.
 *
 * @param {*} start - Selection or range start point.
 * @param {*} end - Selection or range end point.
 * @returns {object} Derived data structure for downstream use.
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
 * Handles rectangles intersect.
 *
 * @param {*} firstRect - Value for first rect.
 * @param {*} secondRect - Value for second rect.
 * @returns {*} Result produced by the helper.
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
 * Normalizes selection ids.
 *
 * @param {*} widgetIds - Value for widget ids.
 * @param {*} orderedWidgetIds - Value for ordered widget ids.
 * @returns {*} Derived data structure for downstream use.
 */
export function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set(widgetIds.filter(Boolean))
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

/**
 * Returns primary selection id.
 *
 * @param {*} widgetIds - Value for widget ids.
 * @param {*} preferredId - Value for preferred id.
 * @returns {*} Requested value or structure.
 */
export function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}

/**
 * Returns widget id from target.
 *
 * @param {*} target - Target object, element, or value being updated.
 * @returns {*} Requested value or structure.
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
 * Returns widget scene origin used by editor DOM positioning.
 *
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} draft - Optional live widget draft.
 * @param {*} visualBounds - Optional tight widget bounds relative to config x/y.
 * @returns {{x: number, y: number}} Widget visual origin.
 */
export function getWidgetSceneOrigin(
  widget,
  draft = null,
  visualBounds = null,
  { boundsScale = 1 } = {},
) {
  const data = draft ? { ...widget.data, ...draft } : widget.data
  const x = data.x ?? 0
  const gradientYOffset =
    widget.type === 'gradient' ? Math.min(0, -(data.value_offset ?? 0)) : 0
  const boundsOffsetX = (visualBounds?.minX ?? 0) * boundsScale
  const boundsOffsetY = (visualBounds?.minY ?? 0) * boundsScale

  return {
    x: x + boundsOffsetX,
    y: (data.y ?? 0) + gradientYOffset + boundsOffsetY,
  }
}

/**
 * Applies live widget styles.
 *
 * @param {*} target - Target object, element, or value being updated.
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} draft - Value for draft.
 * @param {*} globalScale - Scale factor applied to the overlay preview.
 * @returns {*} Result produced by the helper.
 */
export function applyLiveWidgetStyles(target, widget, draft, globalScale) {
  if (!target || !widget) {
    return
  }

  const visualBounds = getWidgetVisualBoundsFromTarget(target)
  const origin = getWidgetSceneOrigin(widget, draft, visualBounds, {
    boundsScale: widget.category === 'plots' ? 1 : globalScale,
  })
  const nextWidth = draft.width ?? widget.data.width
  const nextHeight = draft.height ?? widget.data.height
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const isPlotWidget = widget.category === 'plots'
  const renderScale = isPlotWidget ? globalScale || 1 : 1
  const nextScale = (draft.scale ?? 1) * (isPlotWidget ? 1 : globalScale)

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
 * Applies live scale position styles.
 *
 * @param {*} target - Target object, element, or value being updated.
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} draft - Value for draft.
 * @param {*} globalScale - Scale factor applied to the overlay preview.
 * @returns {*} Result produced by the helper.
 */
export function applyLiveScalePositionStyles(
  target,
  widget,
  draft,
  globalScale,
) {
  if (!target || !widget) {
    return
  }

  const visualBounds = getWidgetVisualBoundsFromTarget(target)
  const draftOrigin = getWidgetSceneOrigin(widget, draft, visualBounds, {
    boundsScale: widget.category === 'plots' ? 1 : globalScale,
  })
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const transforms = []

  if (nextRotation) {
    transforms.push(`rotate(${nextRotation}deg)`)
  }

  if (globalScale !== 1) {
    transforms.push(`scale(${globalScale})`)
  }

  target.style.left = `${draftOrigin.x}px`
  target.style.top = `${draftOrigin.y}px`
  target.style.transform = transforms.join(' ')
}

/**
 * Builds scaled widget data draft.
 *
 * @param {*} origin - Value for origin.
 * @param {*} scaleFactor - Value for scale factor.
 * @returns {object} Derived data structure for downstream use.
 */
export function buildScaledWidgetDataDraft(origin, scaleFactor, widget) {
  const nextFontSize = clamp(
    Math.round((origin.fontSize || 60) * scaleFactor),
    8,
    400,
  )
  const nextIconSize = clamp(
    Math.round((origin.iconSize || 28) * scaleFactor),
    0,
    400,
  )
  const nextIconOffsetX = Math.round((origin.iconOffsetX || 0) * scaleFactor)
  const nextIconOffsetY = Math.round((origin.iconOffsetY || 0) * scaleFactor)
  const nextTriangleWidth = clamp(
    Math.round(
      (origin.triangleWidth ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH) * scaleFactor,
    ),
    0,
    600,
  )
  const nextValueOffset = Math.round((origin.valueOffset || 0) * scaleFactor)

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
