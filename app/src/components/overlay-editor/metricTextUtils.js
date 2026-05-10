/**
 * Provides overlay editor helpers for metric text utils.
 */

import { FONT_FAMILY_MAP } from './constants'

export const METRIC_WIDGET_LINE_HEIGHT = 0.92
export const METRIC_WIDGET_OUTER_GAP_PX = 8
export const METRIC_WIDGET_UNITS_GAP_PX = 8
export const GRADIENT_WIDGET_TRIANGLE_GAP_PX = 8
export const GRADIENT_ZERO_EPSILON = 0.05
export const MAX_GRADIENT_ABS_PERCENT = 25
export const GRADIENT_ZERO_LINE_WIDTH_PX = 1

/**
 * Returns preview font family.
 *
 * @param {*} fontName - Selected font asset name.
 * @returns {*} Requested value or structure.
 */
export function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || fontName || FONT_FAMILY_MAP['Arial.ttf']
}

let metricMeasureContext = null
const NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT = '0123456789-:.%'

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

/**
 * Returns metric measure context.
 * @returns {*} Requested value or structure.
 */
function getMetricMeasureContext() {
  if (metricMeasureContext) {
    return metricMeasureContext
  }

  const canvas = document.createElement('canvas')
  metricMeasureContext = canvas.getContext('2d')
  return metricMeasureContext
}

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Measures preview text.
 *
 * @param {*} text - Text content to measure or render.
 * @param {*} fontSize - Numeric font size value.
 * @param {*} fontFamily - Font family used for measurement or rendering.
 * @returns {object} Computed measurement details.
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

  return /^[0-9:.%+-]+$/.test(text)
    ? NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT
    : text
}

function getPreviewVerticalMetrics(text, fontSize, fontFamily) {
  const metricsText = resolvePreviewVerticalMetricsText(text)
  if (!metricsText) {
    return createEmptyVerticalMetrics()
  }

  const { glyphHeight, ascent, descent } = measurePreviewText(
    metricsText,
    fontSize,
    fontFamily,
  )
  return {
    glyphHeight,
    ascent,
    descent,
  }
}

/**
 * Returns preview text baseline.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.top - Value for top.
 * @param {*} options.lineHeight - Numeric line height value.
 * @param {*} options.ascent - Value for ascent.
 * @param {*} options.glyphHeight - Numeric glyph height value.
 * @returns {*} Requested value or structure.
 */
export function getPreviewTextBaseline({
  top = 0,
  lineHeight,
  ascent,
  glyphHeight,
}) {
  if (!glyphHeight) {
    return top + lineHeight
  }

  return top + ((lineHeight - glyphHeight) / 2 + ascent)
}

/**
 * Returns metric widget layout.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.fontSize - Numeric font size value.
 * @param {*} options.fontFamily - Font family used for measurement or rendering.
 * @param {*} options.valueText - Formatted value text shown in the widget.
 * @param {*} options.unitText - Value for unit text.
 * @param {*} options.showIcon - Boolean flag for show icon.
 * @param {*} options.showUnits - Boolean flag for show units.
 * @param {*} options.iconSize - Numeric icon size value.
 * @returns {object} Requested value or structure.
 */
export function getMetricWidgetLayout({
  fontSize,
  fontFamily,
  valueText,
  unitText,
  showIcon,
  showUnits,
  iconSize,
}) {
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const unitsFontSize = Math.max(fontSize * 0.28, 12)
  const unitsLineHeight = unitsFontSize * METRIC_WIDGET_LINE_HEIGHT
  const iconMarginRight = Math.max(fontSize * 0.08, 8)
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(
    valueText,
    fontSize,
    fontFamily,
  )
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText
    ? measurePreviewText(unitText, unitsFontSize, fontFamily)
    : createEmptyTextMeasure()
  const unitsVerticalMetrics = showUnitText
    ? getPreviewVerticalMetrics(unitText, unitsFontSize, fontFamily)
    : createEmptyVerticalMetrics()
  const textGroupHeight = showUnitText
    ? Math.max(valueLineHeight, unitsLineHeight)
    : valueLineHeight
  const rowHeight = Math.max(showIcon ? iconSize : 0, textGroupHeight)
  const textGroupLeft = showIcon
    ? iconSize + METRIC_WIDGET_OUTER_GAP_PX + iconMarginRight
    : 0
  const textGroupTop = (rowHeight - textGroupHeight) / 2
  const textGroupBottom = textGroupTop + textGroupHeight
  const valueTop =
    textGroupBottom - (valueLineHeight + valueVerticalMetrics.glyphHeight) / 2
  const unitsTop =
    textGroupBottom - (unitsLineHeight + unitsVerticalMetrics.glyphHeight) / 2
  const unitsLeft =
    textGroupLeft + valueMeasure.width + METRIC_WIDGET_UNITS_GAP_PX
  const width = showUnitText
    ? unitsLeft + unitsMeasure.width
    : textGroupLeft + valueMeasure.width

  return {
    icon: showIcon
      ? {
          left: 0,
          top: (rowHeight - iconSize) / 2,
          size: iconSize,
        }
      : null,
    value: {
      left: textGroupLeft,
      top: valueTop,
      baseline: getPreviewTextBaseline({
        top: valueTop,
        lineHeight: valueLineHeight,
        ascent: valueVerticalMetrics.ascent,
        descent: valueVerticalMetrics.descent,
        glyphHeight: valueVerticalMetrics.glyphHeight,
      }),
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
 * Returns the actual painted metric widget bounds.
 *
 * @param {object} layout - Base metric widget layout.
 * @param {object} options - Bounds options.
 * @param {*} options.iconOffsetX - Horizontal icon offset.
 * @param {*} options.iconOffsetY - Vertical icon offset.
 * @returns {object} Tight visual bounds and inner-content offsets.
 */
export function getMetricWidgetVisualBounds(
  layout,
  { iconOffsetX = 0, iconOffsetY = 0 } = {},
) {
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
    bounds = expandMetricBounds(
      bounds,
      iconLeft,
      iconTop,
      iconLeft + layout.icon.size,
      iconTop + layout.icon.size,
    )
  }

  ;[layout.value, layout.units]
    .map(getPreviewTextVisualBounds)
    .filter(Boolean)
    .forEach((segmentBounds) => {
      bounds = expandMetricBounds(
        bounds,
        segmentBounds.left,
        segmentBounds.top,
        segmentBounds.right,
        segmentBounds.bottom,
      )
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

/**
 * Returns widget opacity.
 *
 * @param {*} data - Widget or API data used by the helper.
 * @param {*} globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {*} Requested value or structure.
 */
export function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}

/**
 * Returns text shadow.
 *
 * @param {*} data - Widget or API data used by the helper.
 * @returns {*} Requested value or structure.
 */
export function getTextShadow(data) {
  const shadow = getTextShadowParts(data)

  if (!shadow) return undefined

  return `${shadow.distance}px ${shadow.distance}px ${shadow.strength}px ${shadow.color}`
}

/**
 * Returns text shadow parts.
 *
 * @param {*} data - Widget or API data used by the helper.
 * @returns {*} Requested value or structure.
 */
export function getTextShadowParts(data) {
  const shadowStrength = Number(data?.shadow_strength) || 0
  const shadowDistance = Number(data?.shadow_distance) || 0
  const shadowColor = data?.shadow_color

  if (!shadowColor || (!shadowStrength && !shadowDistance)) return undefined

  return {
    color: shadowColor,
    distance: shadowDistance,
    strength: shadowStrength,
  }
}

/**
 * Returns text outline shadow.
 *
 * @param {*} data - Widget or API data used by the helper.
 * @returns {*} Requested value or structure.
 */
export function getTextOutlineShadow(data) {
  const borderThickness = Number(data?.border_thickness) || 0
  const borderColor = data?.border_color

  if (!borderThickness || !borderColor) return ''

  const offsets = [
    [-1, 0],
    [1, 0],
    [0, -1],
    [0, 1],
    [-1, -1],
    [1, -1],
    [-1, 1],
    [1, 1],
  ]
  const layers = []

  for (let step = 1; step <= borderThickness; step += 1) {
    offsets.forEach(([x, y]) => {
      layers.push(`${x * step}px ${y * step}px 0 ${borderColor}`)
    })
  }

  return layers.join(', ')
}

/**
 * Returns combined text shadow.
 *
 * @param {*} data - Widget or API data used by the helper.
 * @returns {*} Requested value or structure.
 */
export function getCombinedTextShadow(data) {
  const outlineShadow = getTextOutlineShadow(data)
  const dropShadow = getTextShadow(data)

  if (outlineShadow && dropShadow) {
    return `${outlineShadow}, ${dropShadow}`
  }

  return outlineShadow || dropShadow || undefined
}

/**
 * Formats speed.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} unit - Selected measurement unit.
 * @returns {object} Formatted representation of the input.
 */
export function formatSpeed(value, unit) {
  const conversions = {
    kmh: { units: 'KM/H', factor: 3.6 },
    mph: { units: 'MPH', factor: 2.236936 },
    kn: { units: 'KN', factor: 1.943844 },
    mps: { units: 'M/S', factor: 1 },
  }
  const selection = conversions[unit] || conversions.kmh

  if (value === null || value === undefined) {
    return { value: '--', units: selection.units }
  }

  const numericValue = Number(value)
  return {
    value: Math.round(numericValue * selection.factor).toString(),
    units: selection.units,
  }
}

/**
 * Formats temperature.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} unit - Selected measurement unit.
 * @returns {object} Formatted representation of the input.
 */
export function formatTemperature(value, unit) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units: unit === 'fahrenheit' ? '\u00B0F' : '\u00B0C',
    }
  }

  const numericValue = Number(value)
  if (unit === 'fahrenheit') {
    return {
      value: Math.round((numericValue * 9) / 5 + 32).toString(),
      units: '\u00B0F',
    }
  }

  return {
    value: Math.round(numericValue).toString(),
    units: '\u00B0C',
  }
}

/**
 * Pads a numeric value with a leading zero when needed.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Result produced by the helper.
 */
function padNumber(value) {
  return String(value).padStart(2, '0')
}

/**
 * Formats time value.
 *
 * @param {*} format - Formatting mode or template key.
 * @param {*} timestamp - Value for timestamp.
 * @returns {string} Formatted representation of the input.
 */
export function formatTimeValue(format, timestamp) {
  if (!timestamp) return '--:--'

  const date = new Date(timestamp)
  if (!Number.isFinite(date.getTime())) return '--:--'

  const day = padNumber(date.getDate())
  const month = padNumber(date.getMonth() + 1)
  const year = date.getFullYear()
  const shortMonth = date
    .toLocaleString('en-US', { month: 'short' })
    .toUpperCase()
  const longMonth = date
    .toLocaleString('en-US', { month: 'long' })
    .toUpperCase()
  const hour24 = padNumber(date.getHours())
  const hour12Raw = date.getHours() % 12 || 12
  const hour12 = padNumber(hour12Raw)
  const minutes = padNumber(date.getMinutes())
  const suffix = date.getHours() >= 12 ? 'PM' : 'AM'

  const formatMap = {
    'date-dd-mm-yyyy': `${day}-${month}-${year}`,
    'date-mm-dd-yyyy': `${month}-${day}-${year}`,
    'date-yyyy-mm-dd': `${year}-${month}-${day}`,
    'date-dd-mmm-yyyy': `${day} ${shortMonth} ${year}`,
    'date-mmm-dd-yyyy': `${shortMonth} ${day} ${year}`,
    'date-dd-mmmm-yyyy': `${day} ${longMonth} ${year}`,
    'date-mmmm-dd-yyyy': `${longMonth} ${day} ${year}`,
    'time-24': `${hour24}:${minutes}`,
    'time-12': `${hour12}:${minutes} ${suffix}`,
    'date-time-24': `${day}-${month}-${year} ${hour24}:${minutes}`,
    'date-time-12': `${day}-${month}-${year} ${hour12}:${minutes} ${suffix}`,
    'date-mmm-time-24': `${day} ${shortMonth} ${hour24}:${minutes}`,
    'date-mmm-time-12': `${day} ${shortMonth} ${hour12}:${minutes} ${suffix}`,
    'date-mmmm-time-24': `${day} ${longMonth} ${hour24}:${minutes}`,
    'date-mmmm-time-12': `${day} ${longMonth} ${hour12}:${minutes} ${suffix}`,
  }

  return formatMap[format] || formatMap['time-24']
}

/**
 * Formats gradient value.
 *
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} value - Input value processed by the helper.
 * @returns {string} Formatted representation of the input.
 */
export function formatGradientValue(widget, value) {
  if (value === null || value === undefined) return '--'

  const decimals = widget.data.decimals ?? 0
  const numericValue = Number(value)
  const absoluteValue = Math.abs(numericValue).toFixed(decimals)
  const sign = numericValue > 0 ? '+' : numericValue < 0 ? '-' : ''
  const prefix = widget.data.show_sign === false ? '' : sign

  return `${prefix}${absoluteValue}`
}

/**
 * Returns gradient triangle height.
 *
 * @param {*} value - Gradient value in percent.
 * @param {*} width - Numeric width value.
 * @returns {number} Derived height for the triangle indicator.
 */
export function getGradientTriangleHeight(value, width) {
  const safeWidth = Math.max(Number(width) || 0, 0)
  if (safeWidth <= 0) {
    return 0
  }

  const numericValue = Number(value)
  if (!Number.isFinite(numericValue)) {
    return 0
  }

  const magnitude = Math.min(Math.abs(numericValue), MAX_GRADIENT_ABS_PERCENT)
  if (magnitude <= GRADIENT_ZERO_EPSILON) {
    return 0
  }

  const halfAngleRadians = (magnitude * 0.5 * Math.PI) / 180
  return safeWidth * Math.tan(halfAngleRadians)
}

/**
 * Returns whether a gradient value should be treated as zero.
 *
 * @param {*} value - Gradient value in percent.
 * @returns {boolean} Whether the indicator should use the zero baseline state.
 */
export function isGradientZero(value) {
  const numericValue = Number(value)
  return (
    !Number.isFinite(numericValue) ||
    Math.abs(numericValue) <= GRADIENT_ZERO_EPSILON
  )
}

/**
 * Returns gradient widget layout.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.fontSize - Numeric font size value.
 * @param {*} options.fontFamily - Font family used for measurement or rendering.
 * @param {*} options.valueText - Formatted value text shown in the widget.
 * @param {*} options.valueOffset - Vertical offset applied to the value anchor.
 * @param {*} options.gradientValue - Current gradient numeric value.
 * @param {*} options.triangleWidth - Configured triangle width.
 * @param {*} options.showTriangle - Boolean flag for triangle visibility.
 * @returns {object} Requested value or structure.
 */
export function getGradientWidgetLayout({
  fontSize,
  fontFamily,
  valueText,
  valueOffset,
  gradientValue,
  triangleWidth,
  showTriangle,
  scale,
}) {
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const safeValueOffset = (Number(valueOffset) || 0) / (scale || 1)
  const safeTriangleWidth = Math.max(Number(triangleWidth) || 0, 0)
  const maxTriangleHeight =
    showTriangle && safeTriangleWidth > 0
      ? getGradientTriangleHeight(MAX_GRADIENT_ABS_PERCENT, safeTriangleWidth)
      : 0
  const triangleHeight =
    showTriangle && safeTriangleWidth > 0
      ? getGradientTriangleHeight(gradientValue, safeTriangleWidth)
      : 0
  const indicatorVisible = showTriangle && safeTriangleWidth > 0
  const contentWidth = Math.max(
    valueMeasure.width,
    indicatorVisible ? safeTriangleWidth : 0,
  )
  const indicatorTop = valueLineHeight + GRADIENT_WIDGET_TRIANGLE_GAP_PX
  const zeroBaseline = indicatorTop + maxTriangleHeight
  const anchoredValueTop = -safeValueOffset
  const indicatorHeight = indicatorVisible ? maxTriangleHeight * 2 : 0
  const rawMinY = Math.min(0, anchoredValueTop)
  const rawMaxY = Math.max(
    anchoredValueTop + valueLineHeight,
    indicatorVisible
      ? indicatorTop + indicatorHeight
      : anchoredValueTop + valueLineHeight,
  )
  const baseline = getPreviewTextBaseline({
    top: anchoredValueTop,
    lineHeight: valueLineHeight,
    ascent: valueMeasure.ascent,
    descent: valueMeasure.descent,
    glyphHeight: valueMeasure.glyphHeight,
  })

  const yOffset = rawMinY

  return {
    width: contentWidth,
    height: rawMaxY - rawMinY,
    yOffset,
    value: {
      left: (contentWidth - valueMeasure.width) / 2,
      top: anchoredValueTop - yOffset,
      baseline: baseline - yOffset,
      width: valueMeasure.width,
      lineHeight: valueLineHeight,
    },
    triangle: indicatorVisible
      ? {
          left: (contentWidth - safeTriangleWidth) / 2,
          top: indicatorTop - yOffset,
          width: safeTriangleWidth,
          height: triangleHeight,
          maxHeight: maxTriangleHeight,
          baseline: zeroBaseline - yOffset,
          isZero: isGradientZero(gradientValue),
        }
      : null,
  }
}

/**
 * Builds gradient triangle path.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @returns {*} Derived data structure for downstream use.
 */
export function buildGradientTrianglePath(value, width, height) {
  const safeWidth = Math.max(Number(width) || 0, 0)
  const safeHeight = Math.max(Number(height) || 0, 0)
  const numericValue = Number(value)

  if (
    safeWidth <= 0 ||
    safeHeight <= 0 ||
    !Number.isFinite(numericValue) ||
    Math.abs(numericValue) <= GRADIENT_ZERO_EPSILON
  ) {
    return ''
  }

  if (numericValue > 0) {
    return `M 0 0 L ${safeWidth} 0 L ${safeWidth} ${-safeHeight} Z`
  }

  return `M 0 0 L ${safeWidth} 0 L ${safeWidth} ${safeHeight} Z`
}
