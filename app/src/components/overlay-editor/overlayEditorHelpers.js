import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import { buildWidgetTransform, clamp } from './utils'

export function clearLiveWidgetDraft(draftWidgetsRef, widgetId) {
  delete draftWidgetsRef.current[widgetId]
}

export function clearLiveWidgetDrafts(draftWidgetsRef, widgetIds) {
  widgetIds.forEach((widgetId) => {
    delete draftWidgetsRef.current[widgetId]
  })
}

export function isEditableElement(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(
    target.closest('input, textarea, select, [contenteditable="true"]'),
  )
}

export function hasSelectionModifier(event) {
  return event.metaKey || event.ctrlKey || event.shiftKey
}

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

export function rectanglesIntersect(firstRect, secondRect) {
  return !(
    firstRect.x + firstRect.width < secondRect.x ||
    secondRect.x + secondRect.width < firstRect.x ||
    firstRect.y + firstRect.height < secondRect.y ||
    secondRect.y + secondRect.height < firstRect.y
  )
}

export function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set(widgetIds.filter(Boolean))
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

export function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}

export function getWidgetIdFromTarget(target) {
  if (!(target instanceof HTMLElement)) {
    return null
  }

  return target.dataset.widgetId || null
}

export function applyLiveWidgetStyles(target, widget, draft, globalScale) {
  if (!target || !widget) {
    return
  }

  const nextX = draft.x ?? widget.data.x ?? 0
  const nextY = draft.y ?? widget.data.y ?? 0
  const nextWidth = draft.width ?? widget.data.width
  const nextHeight = draft.height ?? widget.data.height
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const nextScale = (draft.scale ?? 1) * globalScale

  target.style.left = `${nextX}px`
  target.style.top = `${nextY}px`

  if (typeof nextWidth === 'number') {
    target.style.width = `${nextWidth}px`
  }

  if (typeof nextHeight === 'number') {
    target.style.height = `${nextHeight}px`
  }

  target.style.transform =
    buildWidgetTransform({
      rotation: nextRotation,
      scale: nextScale,
    }) || ''
}

export function applyLiveScalePositionStyles(
  target,
  widget,
  draft,
  globalScale,
) {
  if (!target || !widget) {
    return
  }

  const baseX = widget.data.x ?? 0
  const baseY = widget.data.y ?? 0
  const nextTranslateX = (draft.x ?? baseX) - baseX
  const nextTranslateY = (draft.y ?? baseY) - baseY
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const transforms = [`translate(${nextTranslateX}px, ${nextTranslateY}px)`]

  if (nextRotation) {
    transforms.push(`rotate(${nextRotation}deg)`)
  }

  if (globalScale !== 1) {
    transforms.push(`scale(${globalScale})`)
  }

  target.style.left = `${baseX}px`
  target.style.top = `${baseY}px`
  target.style.transform = transforms.join(' ')
}

export function buildScaledWidgetDataDraft(origin, scaleFactor) {
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

  return {
    font_size: nextFontSize,
    icon_size: nextIconSize,
    icon_offset_x: nextIconOffsetX,
    icon_offset_y: nextIconOffsetY,
    triangle_width: nextTriangleWidth,
    value_offset: nextValueOffset,
  }
}
