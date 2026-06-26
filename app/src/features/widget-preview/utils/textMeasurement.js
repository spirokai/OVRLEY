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
import { getFontFamilyName } from '@/lib/fonts'
import { clamp } from '@/lib/utils'

let metricMeasureContext = null

function createEmptyTextMeasure() {
  return {
    width: 0,
    glyphHeight: 0,
    ascent: 0,
    descent: 0,
    fontAscent: 0,
    fontDescent: 0,
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

/**
 * Resolves a font name to its CSS font-family value via the FONT_FAMILY_MAP lookup.
 *
 * Falls back to the font name itself if not found in the map, and finally to
 * the first discovered bundled font family when available.
 *
 * @param {string} fontName - Font name key from FONT_FAMILY_MAP or a raw CSS font-family.
 * @returns {string} CSS-compatible font-family string.
 */
export function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || getFontFamilyName(fontName) || 'sans-serif'
}

/**
 * Measures text dimensions using a canvas 2D context.
 *
 * Returns width, glyph bounding box, ascent, and descent using the Canvas API's
 * measureText method to match the Skia renderer's text layout.
 *
 * @param {string} text - Text to measure.
 * @param {number} fontSize - Font size in pixels.
 * @param {string} fontFamily - CSS font family.
 * @returns {{ width: number, glyphHeight: number, ascent: number, descent: number, boundsLeft: number, boundsRight: number }} Measurement results.
 */
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
  const fontAscent = metrics.fontBoundingBoxAscent || ascent
  const fontDescent = metrics.fontBoundingBoxDescent || descent
  const glyphHeight = ascent + descent

  return {
    width: metrics.width,
    glyphHeight,
    ascent,
    descent,
    fontAscent,
    fontDescent,
    boundsLeft: metrics.actualBoundingBoxLeft || 0,
    boundsRight: metrics.actualBoundingBoxRight || metrics.width,
  }
}

function resolvePreviewVerticalMetricsText(text) {
  if (!text) {
    return ''
  }

  return /^[0-9/:.%+-]+$/.test(text) ? NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT : text
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

/**
 * Computes the SVG text `y` baseline position from vertical metrics.
 *
 * Centers the glyph vertically within the line height while aligning to the
 * alphabetic baseline, matching the Skia renderer's text positioning.
 *
 * @param {object} params
 * @param {number} [params.top=0] - Top of the text area.
 * @param {number} params.lineHeight - Total line height in pixels.
 * @param {number} params.ascent - Glyph ascent from baseline.
 * @param {number} params.glyphHeight - Total glyph height (ascent + descent).
 * @returns {number} Y position for the SVG text baseline attribute.
 */
export function getPreviewTextBaseline({ top = 0, lineHeight, ascent, glyphHeight }) {
  if (!glyphHeight) {
    return top + lineHeight
  }

  return top + ((lineHeight - glyphHeight) / 2 + ascent)
}

/**
 * Computes the full metric widget layout — icon, value text, and units text positions.
 *
 * Calculates positions, baselines, and dimensions for all three visual elements
 * (icon, value, units) based on font metrics and widget configuration.
 *
 * @param {object} params
 * @param {number} params.fontSize - Value font size in pixels.
 * @param {string} params.fontFamily - Font family.
 * @param {string} params.valueText - Value text string.
 * @param {string} params.unitText - Units text string.
 * @param {boolean} params.showIcon - Whether to include an icon element.
 * @param {boolean} params.showUnits - Whether to include units text.
 * @param {number} params.iconSize - Icon size in pixels.
 * @returns {{ icon: object|null, value: object, units: object|null, width: number, height: number, unitsFontSize: number }} Layout positions and dimensions.
 */
export function getMetricWidgetLayout({ fontSize, fontFamily, valueText, unitText, showIcon, showUnits, iconSize }) {
  // Font metrics — compute line heights and measure both value and units text using canvas measurement
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const unitsFontSize = Math.max(fontSize * 0.28, 12)
  const unitsLineHeight = unitsFontSize * METRIC_WIDGET_LINE_HEIGHT
  const iconMarginRight = Math.max(fontSize * 0.08, 8)
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, fontSize, fontFamily)
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText ? measurePreviewText(unitText, unitsFontSize, fontFamily) : createEmptyTextMeasure()
  const unitsVerticalMetrics = showUnitText ? getPreviewVerticalMetrics(unitText, unitsFontSize, fontFamily) : createEmptyVerticalMetrics()

  // Row layout — determine the overall row height based on the tallest element (icon vs text group)
  const textGroupHeight = showUnitText ? Math.max(valueLineHeight, unitsLineHeight) : valueLineHeight
  const rowHeight = Math.max(showIcon ? iconSize : 0, textGroupHeight)
  const textGroupLeft = showIcon ? iconSize + METRIC_WIDGET_OUTER_GAP_PX + iconMarginRight : 0
  const textGroupTop = (rowHeight - textGroupHeight) / 2
  const textGroupBottom = textGroupTop + textGroupHeight

  // Value text baseline — center the glyph vertically within the line height using the alphabetic baseline
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

/**
 * Computes the visual bounding box of a metric widget layout, accounting for icon offsets.
 *
 * Evaluates the actual rendered extents of the icon (with offsets), value text,
 * and units text, then computes the minimal bounding rectangle and alignment offsets.
 *
 * @param {object|null} layout - Layout from getMetricWidgetLayout.
 * @param {object} [params={}] - Offset parameters.
 * @param {number} [params.iconOffsetX=0] - Horizontal icon offset relative to layout.
 * @param {number} [params.iconOffsetY=0] - Vertical icon offset relative to layout.
 * @returns {{ minX: number, minY: number, maxX: number, maxY: number, width: number, height: number, offsetX: number, offsetY: number }} Visual bounds and alignment offsets.
 */
export function getMetricWidgetVisualBounds(layout, { iconOffsetX = 0, iconOffsetY = 0 } = {}) {
  // Empty layout — return zero bounds when no layout is provided
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

  // Initialize bounds to infinity/negative-infinity for expansion
  let bounds = {
    minX: Number.POSITIVE_INFINITY,
    minY: Number.POSITIVE_INFINITY,
    maxX: Number.NEGATIVE_INFINITY,
    maxY: Number.NEGATIVE_INFINITY,
  }

  // Icon bounds — expand the bounding rect to include the icon with user-specified offsets
  if (layout.icon) {
    const iconLeft = layout.icon.left + iconOffsetX
    const iconTop = layout.icon.top + iconOffsetY
    bounds = expandMetricBounds(bounds, iconLeft, iconTop, iconLeft + layout.icon.size, iconTop + layout.icon.size)
  }

  // Text bounds — expand the rect to include value and units text bounding boxes
  ;[layout.value, layout.units]
    .map(getPreviewTextVisualBounds)
    .filter(Boolean)
    .forEach((segmentBounds) => {
      bounds = expandMetricBounds(bounds, segmentBounds.left, segmentBounds.top, segmentBounds.right, segmentBounds.bottom)
    })

  if (!layout.icon) {
    bounds.minX = Math.max(bounds.minX, 0)
  }

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

/**
 * Computes the effective opacity of a widget, combining widget-level and global opacity.
 *
 * Multiplies the widget's individual opacity by the scene's global opacity,
 * clamped to the [0, 1] range.
 *
 * @param {object} data - Widget data object (may contain .opacity).
 * @param {number} [globalOpacity=1] - Global opacity multiplier from the scene.
 * @returns {number} Clamped combined opacity in the 0–1 range.
 */
export function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}
