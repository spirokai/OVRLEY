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
import { FONT_FAMILY_MAP } from '@/features/overlay-editor/data/overlayEditorConfig'
import { getFontFamilyName } from '@/lib/fonts'
import { clamp } from '@/lib/utils'

let metricMeasureContext = null
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
  const fontFamily = FONT_FAMILY_MAP[fontName] ?? getFontFamilyName(fontName)
  if (!fontFamily) throw new Error(`Unknown preview font: ${fontName}`)
  return fontFamily
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
  const iconLeft = layout.icon ? layout.icon.left + iconOffsetX : 0
  const iconTop = layout.icon ? layout.icon.top + iconOffsetY : 0
  const iconRight = layout.icon ? iconLeft + layout.icon.size : 0
  const iconBottom = layout.icon ? iconTop + layout.icon.size : 0
  const minX = layout.icon ? Math.min(0, iconLeft) : 0
  const maxX = layout.icon ? Math.max(layout.width, iconRight) : layout.width
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
