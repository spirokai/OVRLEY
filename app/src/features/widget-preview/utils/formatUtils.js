/**
 * Format utilities — speed, temperature, time, and gradient value formatting
 * as well as gradient widget layout computation.
 */

import {
  METRIC_WIDGET_LINE_HEIGHT,
  GRADIENT_WIDGET_TRIANGLE_GAP_PX,
  MAX_GRADIENT_ABS_PERCENT,
  GRADIENT_ZERO_EPSILON,
} from '@/features/overlay-editor'
import { getStandardMetricDefinition, getStandardMetricDisplayUnit, getStandardMetricUnitLabel } from '@/lib/standard-metrics'
import { measurePreviewText, getPreviewTextBaseline } from './textMeasurement'

/**
 * Formats a speed value into a human-readable string with unit label.
 *
 * @param {number|null|undefined} value - Speed value in meters per second.
 * @param {string} unit - Target unit system ('kmh', 'mph', 'kn', 'mps').
 * @returns {{ value: string, units: string }} Formatted speed string and unit label.
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
 * Formats a temperature value with the specified unit.
 *
 * @param {number|null|undefined} value - Temperature in Celsius.
 * @param {string} unit - Target unit ('celsius' or 'fahrenheit').
 * @returns {{ value: string, units: string }} Formatted temperature string and unit symbol.
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

export function formatPace(value, unit) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units: unit === 'min_per_mi' ? 'MIN/MI' : 'MIN/KM',
    }
  }

  const numericValue = Number(value)
  const totalSeconds = unit === 'min_per_mi' ? numericValue * 1.609344 : numericValue
  const roundedSeconds = Math.max(Math.round(totalSeconds), 0)
  const minutes = Math.floor(roundedSeconds / 60)
  const seconds = roundedSeconds % 60

  return {
    value: `${minutes}:${String(seconds).padStart(2, '0')}`,
    units: unit === 'min_per_mi' ? 'MIN/MI' : 'MIN/KM',
  }
}

function formatRoundedMetric(value, units, decimals = 0) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units,
    }
  }

  const numericValue = Number(value)
  const roundedValue = decimals > 0 ? numericValue.toFixed(decimals).replace(/\.?0+$/, '') : Math.round(numericValue).toString()

  return {
    value: roundedValue,
    units,
  }
}

function convertStandardMetricValue(type, value, displayUnit) {
  const numericValue = Number(value)

  switch (type) {
    case 'g_force':
      return displayUnit === 'mps2' ? numericValue * 9.80665 : numericValue
    case 'air_pressure':
      switch (displayUnit) {
        case 'inhg':
          return numericValue * 29.5299830714
        case 'mmhg':
          return numericValue * 750.061561303
        case 'mbar':
        case 'hpa':
        default:
          return numericValue * 1000
      }
    case 'stride_length':
      switch (displayUnit) {
        case 'cm':
          return numericValue * 100
        case 'ft':
          return numericValue * 3.28084
        case 'in':
          return numericValue * 39.3701
        default:
          return numericValue
      }
    case 'vertical_speed':
      switch (displayUnit) {
        case 'ftmin':
          return numericValue * 196.850394
        case 'mph_vertical':
          return numericValue * 3600
        default:
          return numericValue
      }
    default:
      return numericValue
  }
}

const BALANCE_FORMATS = {
  plain: { valueTemplate: (l, r) => `${l} / ${r}`, placeholder: '-- / --' },
  l_prefix: { valueTemplate: (l, r) => `L${l} / R${r}`, placeholder: '-- / --' },
  percent_label: { valueTemplate: (l, r) => `${l}% / ${r}%`, placeholder: '-- / --' },
  l_suffix: { valueTemplate: (l, r) => `${l}L / ${r}R`, placeholder: '-- / --' },
}

export const BALANCE_FORMAT_OPTIONS = [
  { value: 'percent_label', label: '52% / 48%' },
  { value: 'plain', label: '52 / 48' },
  { value: 'l_prefix', label: 'L52 / R48' },
  { value: 'l_suffix', label: '52L / 48R' },
]

function formatBalance(value, decimals = 0, balanceFormat = 'percent_label') {
  const fmt = BALANCE_FORMATS[balanceFormat] || BALANCE_FORMATS.percent_label

  if (value === null || value === undefined) {
    return {
      value: fmt.placeholder,
      units: '',
    }
  }

  const leftValue = Math.min(Math.max(Number(value), 0), 100)
  const rightValue = Math.min(Math.max(100 - leftValue, 0), 100)
  const leftText = decimals > 0 ? leftValue.toFixed(decimals).replace(/\.?0+$/, '') : Math.round(leftValue).toString()
  const rightText = decimals > 0 ? rightValue.toFixed(decimals).replace(/\.?0+$/, '') : Math.round(rightValue).toString()

  return {
    value: fmt.valueTemplate(leftText, rightText),
    units: '',
  }
}

export function formatStandardMetricDisplay(type, value, widgetData = {}) {
  const definition = getStandardMetricDefinition(type)
  if (!definition) {
    return {
      value: '--',
      units: '',
    }
  }

  const displayUnit = getStandardMetricDisplayUnit(type, widgetData)
  const unitLabel = getStandardMetricUnitLabel(type, displayUnit)

  if (definition.formatter === 'speed') {
    return formatSpeed(value, displayUnit)
  }

  if (definition.formatter === 'temperature') {
    return formatTemperature(value, displayUnit)
  }

  if (definition.formatter === 'pace') {
    return formatPace(value, displayUnit)
  }

  if (definition.formatter === 'balance') {
    return formatBalance(value, widgetData.decimals ?? 0, widgetData.balance_format)
  }

  return formatRoundedMetric(
    value === null || value === undefined ? value : convertStandardMetricValue(type, value, displayUnit),
    unitLabel,
    widgetData.decimals ?? (definition.formatter === 'decimal' ? 1 : 0),
  )
}

function padNumber(value) {
  return String(value).padStart(2, '0')
}

/**
 * Formats a timestamp into a time/date string based on the specified format key.
 *
 * Supports date formats (dd-mm-yyyy, mm-dd-yyyy, etc.), time formats (12h/24h),
 * and combined date-time formats.
 *
 * @param {string} format - Format key (e.g. 'time-24', 'date-dd-mm-yyyy').
 * @param {number|string|null|undefined} timestamp - Timestamp in milliseconds or ISO string.
 * @returns {string} Formatted time/date string.
 */
export function formatTimeValue(format, timestamp) {
  // Early return — missing or invalid timestamps show a placeholder
  if (!timestamp) return '--:--'

  const date = new Date(timestamp)
  if (!Number.isFinite(date.getTime())) return '--:--'

  // Extract all date/time components for format string composition
  const day = padNumber(date.getDate())
  const month = padNumber(date.getMonth() + 1)
  const year = date.getFullYear()
  const shortMonth = date.toLocaleString('en-US', { month: 'short' }).toUpperCase()
  const longMonth = date.toLocaleString('en-US', { month: 'long' }).toUpperCase()
  const hour24 = padNumber(date.getHours())
  const hour12Raw = date.getHours() % 12 || 12
  const hour12 = padNumber(hour12Raw)
  const minutes = padNumber(date.getMinutes())
  const suffix = date.getHours() >= 12 ? 'PM' : 'AM'

  // Format map — selects the rendered string based on the format key; falls back to 24-hour time
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
 * Formats a gradient value as a signed percentage string.
 *
 * @param {object} widget - Widget configuration containing decimal precision and sign display settings.
 * @param {number|null|undefined} value - Raw gradient value.
 * @returns {string} Formatted gradient string with optional sign prefix.
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
 * Computes the height of a gradient indicator triangle for a given value and width.
 *
 * Uses trigonometric relationship between the gradient angle and the triangle width
 * to determine the visual height of the direction indicator.
 *
 * @param {number} value - Gradient value (percent).
 * @param {number} width - Triangle width in pixels.
 * @returns {number} Triangle height in pixels.
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
 * Checks whether a gradient value is effectively zero (within GRADIENT_ZERO_EPSILON).
 *
 * @param {number} value - Gradient value.
 * @returns {boolean} True if the value is zero, non-finite, or within epsilon.
 */
export function isGradientZero(value) {
  const numericValue = Number(value)
  return !Number.isFinite(numericValue) || Math.abs(numericValue) <= GRADIENT_ZERO_EPSILON
}

/**
 * Computes the full layout for a gradient widget — value text and triangle indicator positions.
 *
 * Calculates the dimensions and positions of the value text and optional triangle
 * direction indicator based on font metrics, gradient magnitude, and widget settings.
 *
 * @param {object} params
 * @param {number} params.fontSize - Font size for the value text.
 * @param {string} params.fontFamily - Font family.
 * @param {string} params.valueText - Formatted value string.
 * @param {number} params.valueOffset - Vertical offset for the value text.
 * @param {number} params.gradientValue - Current gradient value.
 * @param {number} params.triangleWidth - Width of the gradient triangle.
 * @param {boolean} params.showTriangle - Whether to render the triangle indicator.
 * @param {number} params.scale - Global scale factor.
 * @returns {{ width: number, height: number, yOffset: number, value: object, triangle: object|null }} Layout dimensions and positioned elements.
 */
export function getGradientWidgetLayout({ fontSize, fontFamily, valueText, valueOffset, gradientValue, triangleWidth, showTriangle, scale }) {
  // Value text measurement — compute line height and measure the value text for positioning
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const safeValueOffset = (Number(valueOffset) || 0) / (scale || 1)
  const safeTriangleWidth = Math.max(Number(triangleWidth) || 0, 0)

  // Triangle dimensions — compute the max possible height and actual height based on gradient magnitude
  const maxTriangleHeight = showTriangle && safeTriangleWidth > 0 ? getGradientTriangleHeight(MAX_GRADIENT_ABS_PERCENT, safeTriangleWidth) : 0
  const triangleHeight = showTriangle && safeTriangleWidth > 0 ? getGradientTriangleHeight(gradientValue, safeTriangleWidth) : 0
  const indicatorVisible = showTriangle && safeTriangleWidth > 0
  const contentWidth = Math.max(valueMeasure.width, indicatorVisible ? safeTriangleWidth : 0)
  const indicatorTop = valueLineHeight + GRADIENT_WIDGET_TRIANGLE_GAP_PX
  const zeroBaseline = indicatorTop + maxTriangleHeight
  const anchoredValueTop = -safeValueOffset
  const indicatorHeight = indicatorVisible ? maxTriangleHeight * 2 : 0

  // Content bounding box — compute the raw vertical extent of value + indicator, then calculate baseline
  const rawMinY = Math.min(0, anchoredValueTop)
  const rawMaxY = Math.max(anchoredValueTop + valueLineHeight, indicatorVisible ? indicatorTop + indicatorHeight : anchoredValueTop + valueLineHeight)
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
 * Builds an SVG path string for a gradient direction triangle.
 *
 * Positive values produce an upward-pointing triangle; negative values produce
 * downward. Zero or non-finite values return an empty string.
 *
 * @param {number} value - Gradient value (determines triangle direction).
 * @param {number} width - Triangle width in pixels.
 * @param {number} height - Triangle height in pixels.
 * @returns {string} SVG path 'd' attribute value, or empty string.
 */
export function buildGradientTrianglePath(value, width, height) {
  const safeWidth = Math.max(Number(width) || 0, 0)
  const safeHeight = Math.max(Number(height) || 0, 0)
  const numericValue = Number(value)

  if (safeWidth <= 0 || safeHeight <= 0 || !Number.isFinite(numericValue) || Math.abs(numericValue) <= GRADIENT_ZERO_EPSILON) {
    return ''
  }

  if (numericValue > 0) {
    return `M 0 0 L ${safeWidth} 0 L ${safeWidth} ${-safeHeight} Z`
  }

  return `M 0 0 L ${safeWidth} 0 L ${safeWidth} ${safeHeight} Z`
}
