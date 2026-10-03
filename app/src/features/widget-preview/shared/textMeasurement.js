/**
 * Text measurement utilities — canvas-based font measurement, metric widget
 * layout computation, and visual bounds calculation.
 */

import {
  METRIC_WIDGET_LINE_HEIGHT,
  METRIC_WIDGET_OUTER_GAP_PX,
  METRIC_WIDGET_UNITS_GAP_PX,
  NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT,
} from '@/features/overlay-editor/data/overlayEditorConstants'
import { clamp } from '@/lib/utils'
import { WIDGET_FONT_WEIGHT } from '@/lib/widget/standard-widgets'

let metricMeasureContext = null
const graphemeSegmenter = new Intl.Segmenter('und', { granularity: 'grapheme' })
const COORDINATE_PREVIEW_VERTICAL_METRICS_TEXT = 'NSEW88\u00B088.888\u203288\u2033'

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
  canvas.style.fontOpticalSizing = 'none'
  canvas.style.fontSynthesis = 'style'
  metricMeasureContext = canvas.getContext('2d')
  return metricMeasureContext
}

/**
 * Returns the CSS family registered from the canonical font catalog.
 * @param {string} fontName - Canonical font ID.
 * @returns {string} CSS-compatible font-family string.
 */
export function getPreviewFontFamily(fontName) {
  return JSON.stringify(`OVRLEY ${fontName}`)
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
 * @param {number} [fontWeight=WIDGET_FONT_WEIGHT] - Supported weight; labels pass their resolved weight.
 * @param {string} [fontStyle='normal'] - Resolved font style; style synthesis is permitted.
 * @param {number} [letterSpacing=0] - Pixels between grapheme clusters, without a trailing gap.
 * @returns {{ width: number, glyphHeight: number, ascent: number, descent: number, boundsLeft: number, boundsRight: number }} Measurement results.
 */
export function measurePreviewText(text, fontSize, fontFamily, fontWeight = WIDGET_FONT_WEIGHT, fontStyle = 'normal', letterSpacing = 0) {
  if (!text) {
    return createEmptyTextMeasure()
  }

  const context = getMetricMeasureContext()
  if (!context) {
    return createEmptyTextMeasure()
  }

  context.font = `${fontStyle === 'normal' ? '' : `${fontStyle} `}${fontWeight} ${fontSize}px ${fontFamily}`
  if (letterSpacing !== 0) return measureSpacedText(text, context, letterSpacing)
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
    boundsRight: metrics.actualBoundingBoxRight ?? metrics.width,
  }
}

// Nonzero spacing lays out intact grapheme runs in both canvas and SVG.
// Advances may become negative; ink bounds are the union of all positioned runs.
function measureSpacedText(text, context, letterSpacing) {
  const runs = []
  let width = 0
  let minX = Infinity
  let maxX = -Infinity
  let ascent = -Infinity
  let descent = -Infinity
  let fontAscent = 0
  let fontDescent = 0
  for (const { segment } of graphemeSegmenter.segment(text)) {
    if (runs.length > 0) width += letterSpacing
    const metrics = context.measureText(segment)
    runs.push({ text: segment, x: width })
    if (metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent > 0) {
      minX = Math.min(minX, width - metrics.actualBoundingBoxLeft)
      maxX = Math.max(maxX, width + metrics.actualBoundingBoxRight)
      ascent = Math.max(ascent, metrics.actualBoundingBoxAscent)
      descent = Math.max(descent, metrics.actualBoundingBoxDescent)
    }
    fontAscent = Math.max(fontAscent, metrics.fontBoundingBoxAscent ?? metrics.actualBoundingBoxAscent)
    fontDescent = Math.max(fontDescent, metrics.fontBoundingBoxDescent ?? metrics.actualBoundingBoxDescent)
    width += metrics.width
  }
  const hasInk = minX !== Infinity
  if (!hasInk) {
    ascent = 0
    descent = 0
  }
  return {
    width,
    glyphHeight: ascent + descent,
    ascent,
    descent,
    fontAscent,
    fontDescent,
    boundsLeft: hasInk ? -minX : 0,
    boundsRight: hasInk ? maxX : 0,
    runs,
  }
}

/** Measures arc text using the Skia-compatible left-bound convention. */
export function measureArcPreviewText(text, fontSize, fontFamily) {
  const measurement = measurePreviewText(text, fontSize, fontFamily)
  return { ...measurement, boundsLeft: -measurement.boundsLeft }
}

function resolvePreviewVerticalMetricsText(text) {
  if (!text) {
    return ''
  }

  if (text.includes('\u00B0') && (text.includes('\u2032') || text.includes('\u2033'))) {
    return COORDINATE_PREVIEW_VERTICAL_METRICS_TEXT
  }

  return text === 'N' || /^[0-9/:.%+-]+$/.test(text) ? NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT : text
}

export function getPreviewVerticalMetrics(text, fontSize, fontFamily) {
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
 * Resolves the natural-width row origin from its alignment and x anchor.
 *
 * @param {'left'|'center'|'right'} contentAlignment
 * @param {number} anchorX
 * @param {number} contentWidth
 * @returns {number}
 */
export function getContentAlignmentOrigin(contentAlignment, anchorX, contentWidth) {
  if (contentAlignment === 'left') return anchorX
  if (contentAlignment === 'center') return anchorX - contentWidth / 2
  if (contentAlignment === 'right') return anchorX - contentWidth
  throw new Error(`Unsupported content alignment: ${String(contentAlignment)}`)
}

/**
 * Lays out an intrinsic metric row around its x anchor.
 *
 * @param {object} params
 * @param {number} params.fontSize
 * @param {string} params.fontFamily
 * @param {string} params.valueText
 * @param {string} params.unitText
 * @param {boolean} params.showIcon
 * @param {boolean} params.showUnits
 * @param {number} params.iconSize
 * @param {'left'|'center'|'right'} params.contentAlignment
 * @param {number} params.globalScale - Global scale applied when the SVG is rendered.
 * @returns {{ icon: object|null, value: object, units: object|null, width: number, height: number, unitsFontSize: number, rowOriginX: number }}
 */
export function getMetricWidgetLayout({ fontSize, fontFamily, valueText, unitText, showIcon, showUnits, iconSize, contentAlignment, globalScale }) {
  // Font metrics — compute line heights and measure both value and units text using canvas measurement
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const unitsFontSize = Math.max(fontSize * 0.28, 12)
  const unitsLineHeight = unitsFontSize * METRIC_WIDGET_LINE_HEIGHT
  const iconMarginRight = Math.max(fontSize * 0.08, 8)
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, fontSize, fontFamily)
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText ? measurePreviewText(unitText, unitsFontSize, fontFamily) : createEmptyTextMeasure()
  const unitsVerticalMetrics = showUnitText
    ? getPreviewVerticalMetrics(unitText === '\u00B0' ? '\u00B0C' : unitText, unitsFontSize, fontFamily)
    : createEmptyVerticalMetrics()

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
  // Keep right-aligned text on a stable raster phase as its digit count changes.
  const valueLayoutWidth = contentAlignment === 'right' ? Math.round(valueMeasure.width * globalScale) / globalScale : valueMeasure.width
  const unitsLeft = textGroupLeft + valueLayoutWidth + METRIC_WIDGET_UNITS_GAP_PX
  const width = showUnitText ? unitsLeft + unitsMeasure.width : textGroupLeft + valueLayoutWidth
  const rowOriginX = getContentAlignmentOrigin(contentAlignment, 0, width)
  const valueGlyphCenterY = valueBaseline + (valueVerticalMetrics.descent - valueVerticalMetrics.ascent) * 0.5

  return {
    icon: showIcon
      ? {
          left: rowOriginX,
          top: valueGlyphCenterY - iconSize * 0.5,
          size: iconSize,
        }
      : null,
    value: {
      left: rowOriginX + textGroupLeft,
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
          left: rowOriginX + unitsLeft,
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
    rowOriginX,
    unitsFontSize,
  }
}

/**
 * Computes stable layout bounds for a metric widget, accounting for icon offsets.
 *
 * Uses horizontal text advances and vertical glyph ink bounds so fixed-width
 * values do not wobble while the selection height stays visually tight.
 *
 * @param {object|null} layout - Layout from getMetricWidgetLayout.
 * @param {object} [params={}] - Offset parameters.
 * @param {number} [params.iconOffsetX=0] - Horizontal icon offset relative to layout.
 * @param {number} [params.iconOffsetY=0] - Vertical icon offset relative to layout.
 * @returns {{ minX: number, minY: number, maxX: number, maxY: number, width: number, height: number, offsetX: number, offsetY: number }} Stable layout bounds and alignment offsets.
 */
export function getMetricWidgetVisualBounds(layout, { iconOffsetX = 0, iconOffsetY = 0 } = {}) {
  // Use horizontal layout advances rather than ink bounds. Ink widths vary by
  // glyph, even for fixed-width fonts, and would make the selection target wobble.
  const iconLeft = layout.icon ? layout.icon.left + iconOffsetX : layout.rowOriginX
  const iconTop = layout.icon ? layout.icon.top + iconOffsetY : 0
  const iconRight = layout.icon ? iconLeft + layout.icon.size : 0
  const iconBottom = layout.icon ? iconTop + layout.icon.size : 0
  const rowRight = layout.rowOriginX + layout.width
  const minX = layout.icon ? Math.min(layout.rowOriginX, iconLeft) : layout.rowOriginX
  const maxX = layout.icon ? Math.max(rowRight, iconRight) : rowRight
  let minY = layout.value.baseline - layout.value.ascent
  let maxY = layout.value.baseline + layout.value.descent
  if (layout.units) {
    minY = Math.min(minY, layout.units.baseline - layout.units.ascent)
    maxY = Math.max(maxY, layout.units.baseline + layout.units.descent)
  }
  if (layout.icon) {
    minY = Math.min(minY, iconTop)
    maxY = Math.max(maxY, iconBottom)
  }
  const width = Math.max(maxX - minX, 0)
  const height = Math.max(maxY - minY, 0)

  return {
    minX,
    minY,
    maxX,
    maxY,
    width,
    height,
    offsetX: -minX,
    offsetY: -minY,
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
  return clamp(data.opacity * globalOpacity, 0, 1)
}
