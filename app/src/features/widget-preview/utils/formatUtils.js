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
import { measurePreviewText, getPreviewTextBaseline } from './textMeasurement'

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

function padNumber(value) {
  return String(value).padStart(2, '0')
}

export function formatTimeValue(format, timestamp) {
  if (!timestamp) return '--:--'

  const date = new Date(timestamp)
  if (!Number.isFinite(date.getTime())) return '--:--'

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

export function formatGradientValue(widget, value) {
  if (value === null || value === undefined) return '--'

  const decimals = widget.data.decimals ?? 0
  const numericValue = Number(value)
  const absoluteValue = Math.abs(numericValue).toFixed(decimals)
  const sign = numericValue > 0 ? '+' : numericValue < 0 ? '-' : ''
  const prefix = widget.data.show_sign === false ? '' : sign

  return `${prefix}${absoluteValue}`
}

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

export function isGradientZero(value) {
  const numericValue = Number(value)
  return !Number.isFinite(numericValue) || Math.abs(numericValue) <= GRADIENT_ZERO_EPSILON
}

export function getGradientWidgetLayout({ fontSize, fontFamily, valueText, valueOffset, gradientValue, triangleWidth, showTriangle, scale }) {
  const valueLineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const valueMeasure = measurePreviewText(valueText, fontSize, fontFamily)
  const safeValueOffset = (Number(valueOffset) || 0) / (scale || 1)
  const safeTriangleWidth = Math.max(Number(triangleWidth) || 0, 0)
  const maxTriangleHeight = showTriangle && safeTriangleWidth > 0 ? getGradientTriangleHeight(MAX_GRADIENT_ABS_PERCENT, safeTriangleWidth) : 0
  const triangleHeight = showTriangle && safeTriangleWidth > 0 ? getGradientTriangleHeight(gradientValue, safeTriangleWidth) : 0
  const indicatorVisible = showTriangle && safeTriangleWidth > 0
  const contentWidth = Math.max(valueMeasure.width, indicatorVisible ? safeTriangleWidth : 0)
  const indicatorTop = valueLineHeight + GRADIENT_WIDGET_TRIANGLE_GAP_PX
  const zeroBaseline = indicatorTop + maxTriangleHeight
  const anchoredValueTop = -safeValueOffset
  const indicatorHeight = indicatorVisible ? maxTriangleHeight * 2 : 0
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
