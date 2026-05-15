/**
 * Text measurement utilities — canvas-based font measurement, metric widget
 * layout computation, and visual bounds calculation.
 */

import {
  FONT_FAMILY_MAP,
  METRIC_WIDGET_LINE_HEIGHT,
  METRIC_WIDGET_OUTER_GAP_PX,
  METRIC_WIDGET_UNITS_GAP_PX,
  NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT,
} from '@/features/overlay-editor'
import { clamp } from '@/lib/geometryUtils'

let metricMeasureContext = null

function createEmptyTextMeasure() {
  return {
    width: 0,
    glyphHeight: 0,
    ascent: 0,
    descent: 0,
    boundsLeft: 0,
    boundsRight: 0,
  }
}

function createEmptyVerticalMetrics() {
  return {
    glyphHeight: 0,
    ascent: 0,
    descent: 0,
  }
}

function getMetricMeasureContext() {
  if (metricMeasureContext) {
    return metricMeasureContext
  }

  const canvas = document.createElement('canvas')
  metricMeasureContext = canvas.getContext('2d')
  return metricMeasureContext
}

export function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || fontName || FONT_FAMILY_MAP['Arial.ttf']
}

export function measurePreviewText(text, fontSize, fontFamily) {
  if (!text) {
    return createEmptyTextMeasure()
  }

  const context = getMetricMeasureContext()
  if (!context) {
    return createEmptyTextMeasure()
  }

  context.font = `${fontSize}px ${fontFamily}`
  const metrics = context.measureText(text)
  const ascent = metrics.actualBoundingBoxAscent || 0
  const descent = metrics.actualBoundingBoxDescent || 0
  const glyphHeight = ascent + descent

  return {
    width: metrics.width,
    glyphHeight,
    ascent,
    descent,
    boundsLeft: metrics.actualBoundingBoxLeft || 0,
    boundsRight: metrics.actualBoundingBoxRight || metrics.width,
  }
}

function resolvePreviewVerticalMetricsText(text) {
  if (!text) {
    return ''
  }

  return /^[0-9:.%+-]+$/.test(text) ? NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT : text
}

function getPreviewVerticalMetrics(text, fontSize, fontFamily) {
  const metricsText = resolvePreviewVerticalMetricsText(text)
  if (!metricsText) {
    return createEmptyVerticalMetrics()
  }

  const { glyphHeight, ascent, descent } = measurePreviewText(metricsText, fontSize, fontFamily)
  return {
    glyphHeight,
    ascent,
    descent,
  }
}

export function getPreviewTextBaseline({ top = 0, lineHeight, ascent, glyphHeight }) {
  if (!glyphHeight) {
    return top + lineHeight
  }

  return top + ((lineHeight - glyphHeight) / 2 + ascent)
}

export function getMetricWidgetLayout({ fontSize, fontFamily, valueText, unitText, showIcon, showUnits, iconSize }) {
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const unitsFontSize = Math.max(fontSize * 0.28, 12)
  const unitsLineHeight = unitsFontSize * METRIC_WIDGET_LINE_HEIGHT
  const iconMarginRight = Math.max(fontSize * 0.08, 8)
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, fontSize, fontFamily)
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText ? measurePreviewText(unitText, unitsFontSize, fontFamily) : createEmptyTextMeasure()
  const unitsVerticalMetrics = showUnitText ? getPreviewVerticalMetrics(unitText, unitsFontSize, fontFamily) : createEmptyVerticalMetrics()
  const textGroupHeight = showUnitText ? Math.max(valueLineHeight, unitsLineHeight) : valueLineHeight
  const rowHeight = Math.max(showIcon ? iconSize : 0, textGroupHeight)
  const textGroupLeft = showIcon ? iconSize + METRIC_WIDGET_OUTER_GAP_PX + iconMarginRight : 0
  const textGroupTop = (rowHeight - textGroupHeight) / 2
  const textGroupBottom = textGroupTop + textGroupHeight
  const valueTop = textGroupBottom - (valueLineHeight + valueVerticalMetrics.glyphHeight) / 2
  const valueBaseline = getPreviewTextBaseline({
    top: valueTop,
    lineHeight: valueLineHeight,
    ascent: valueVerticalMetrics.ascent,
    glyphHeight: valueVerticalMetrics.glyphHeight,
  })
  const unitsTop = textGroupBottom - (unitsLineHeight + unitsVerticalMetrics.glyphHeight) / 2
  const unitsLeft = textGroupLeft + valueMeasure.width + METRIC_WIDGET_UNITS_GAP_PX
  const width = showUnitText ? unitsLeft + unitsMeasure.width : textGroupLeft + valueMeasure.width
  const valueGlyphCenterY = valueBaseline + (valueVerticalMetrics.descent - valueVerticalMetrics.ascent) * 0.5

  return {
    icon: showIcon
      ? {
          left: 0,
          top: valueGlyphCenterY - iconSize * 0.5,
          size: iconSize,
        }
      : null,
    value: {
      left: textGroupLeft,
      top: valueTop,
      baseline: valueBaseline,
      width: valueMeasure.width,
      lineHeight: valueLineHeight,
      ascent: valueVerticalMetrics.ascent,
      descent: valueVerticalMetrics.descent,
      boundsLeft: valueMeasure.boundsLeft,
      boundsRight: valueMeasure.boundsRight,
    },
    units: showUnitText
      ? {
          left: unitsLeft,
          top: unitsTop,
          baseline: getPreviewTextBaseline({
            top: unitsTop,
            lineHeight: unitsLineHeight,
            ascent: unitsVerticalMetrics.ascent,
            descent: unitsVerticalMetrics.descent,
            glyphHeight: unitsVerticalMetrics.glyphHeight,
          }),
          width: unitsMeasure.width,
          fontSize: unitsFontSize,
          lineHeight: unitsLineHeight,
          ascent: unitsVerticalMetrics.ascent,
          descent: unitsVerticalMetrics.descent,
          boundsLeft: unitsMeasure.boundsLeft,
          boundsRight: unitsMeasure.boundsRight,
        }
      : null,
    width,
    height: rowHeight,
    unitsFontSize,
  }
}

function expandMetricBounds(currentBounds, left, top, right, bottom) {
  return {
    minX: Math.min(currentBounds.minX, left),
    minY: Math.min(currentBounds.minY, top),
    maxX: Math.max(currentBounds.maxX, right),
    maxY: Math.max(currentBounds.maxY, bottom),
  }
}

function getPreviewTextVisualBounds(segment) {
  if (!segment) {
    return null
  }

  const left = segment.left - (segment.boundsLeft ?? 0)
  const top = segment.baseline - (segment.ascent ?? 0)
  const right = segment.left + (segment.boundsRight ?? segment.width ?? 0)
  const bottom = segment.baseline + (segment.descent ?? 0)

  return { left, top, right, bottom }
}

export function getMetricWidgetVisualBounds(layout, { iconOffsetX = 0, iconOffsetY = 0 } = {}) {
  if (!layout) {
    return {
      minX: 0,
      minY: 0,
      maxX: 0,
      maxY: 0,
      width: 0,
      height: 0,
      offsetX: 0,
      offsetY: 0,
    }
  }

  let bounds = {
    minX: Number.POSITIVE_INFINITY,
    minY: Number.POSITIVE_INFINITY,
    maxX: Number.NEGATIVE_INFINITY,
    maxY: Number.NEGATIVE_INFINITY,
  }

  if (layout.icon) {
    const iconLeft = layout.icon.left + iconOffsetX
    const iconTop = layout.icon.top + iconOffsetY
    bounds = expandMetricBounds(bounds, iconLeft, iconTop, iconLeft + layout.icon.size, iconTop + layout.icon.size)
  }

  ;[layout.value, layout.units]
    .map(getPreviewTextVisualBounds)
    .filter(Boolean)
    .forEach((segmentBounds) => {
      bounds = expandMetricBounds(bounds, segmentBounds.left, segmentBounds.top, segmentBounds.right, segmentBounds.bottom)
    })

  if (!Number.isFinite(bounds.minX)) {
    bounds = expandMetricBounds(bounds, 0, 0, layout.width, layout.height)
  }

  const width = Math.max(bounds.maxX - bounds.minX, 0)
  const height = Math.max(bounds.maxY - bounds.minY, 0)

  return {
    ...bounds,
    width,
    height,
    offsetX: -bounds.minX,
    offsetY: -bounds.minY,
  }
}

export function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}
