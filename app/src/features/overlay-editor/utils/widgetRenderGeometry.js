import { buildWidgetTransform } from '@/lib/geometryUtils'
import { isBoxedMetricWidget } from '@/lib/widget/display-type-behavior'
import { resolveActiveMetricWidgetData } from '@/lib/widget/metric-widget-resolver'
import { headingTapeRenderedHeight } from '@/features/widget-preview/utils/headingGeometry'
import { getWidgetSceneOrigin } from './overlayEditorHelpers'

function buildScaleTranslate(tx, ty) {
  if (!tx && !ty) {
    return ''
  }

  return `translate(${tx}px, ${ty}px)`
}

export function resolveWidgetRenderGeometry(widget, visualBounds, globalScale, preview = null) {
  const isBoxed = isBoxedMetricWidget(widget)
  const scaleFactor = preview?.scaleFactor
  const isScaling = Number.isFinite(scaleFactor)
  const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
  const resolvedData = isBoxed ? resolveActiveMetricWidgetData(widget.data) : widget.data
  const boxedWidth = (resolvedData.width ?? 0) * (globalScale || 1)
  const boxedHeight =
    resolvedData.display_type === 'heading_tape'
      ? headingTapeRenderedHeight(resolvedData) * (globalScale || 1)
      : (resolvedData.height ?? 0) * (globalScale || 1)
  const staticOrigin = getWidgetSceneOrigin(widget, null, visualBounds, {
    boundsScale: isBoxed ? 1 : globalScale,
  })

  const left = isScaling ? (preview.left ?? staticOrigin.x) : staticOrigin.x
  const top = isScaling ? (preview.top ?? staticOrigin.y) : staticOrigin.y
  const width = isScaling ? preview.width : isBoxed ? boxedWidth : (visualBounds?.width ?? widget.data.width)
  const height = isScaling ? preview.height : isBoxed ? boxedHeight : (visualBounds?.height ?? widget.data.height)
  const translateX = isScaling ? (preview.translateX ?? 0) : 0
  const translateY = isScaling ? (preview.translateY ?? 0) : 0
  const scale = isScaling ? globalScale * scaleFactor : isBoxed ? 1 : globalScale
  const transformParts = []
  const translate = buildScaleTranslate(translateX, translateY)

  if (translate) {
    transformParts.push(translate)
  }

  const baseTransform = buildWidgetTransform({ scale, rotation })
  if (baseTransform) {
    transformParts.push(baseTransform)
  }

  return {
    badgeLeft: left + translateX,
    badgeTop: top + translateY,
    height,
    isScaling,
    left,
    top,
    transform: transformParts.join(' '),
    translateX,
    translateY,
    width,
  }
}

export function buildRenderedGeometrySignature(widget, visualBounds, globalScale, preview = null) {
  if (!widget) {
    return 'none'
  }

  const renderGeometry = resolveWidgetRenderGeometry(widget, visualBounds, globalScale, preview)

  return JSON.stringify({
    id: widget.id,
    left: renderGeometry.left,
    top: renderGeometry.top,
    width: renderGeometry.width ?? null,
    height: renderGeometry.height ?? null,
    transform: renderGeometry.transform,
    minX: visualBounds?.minX ?? null,
    minY: visualBounds?.minY ?? null,
    maxX: visualBounds?.maxX ?? null,
    maxY: visualBounds?.maxY ?? null,
  })
}
