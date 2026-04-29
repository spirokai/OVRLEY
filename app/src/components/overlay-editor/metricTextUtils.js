import { FONT_FAMILY_MAP } from './constants'

export const METRIC_WIDGET_LINE_HEIGHT = 0.92
export const METRIC_WIDGET_OUTER_GAP_PX = 8
export const METRIC_WIDGET_UNITS_GAP_PX = 8

export function getPreviewFontFamily(fontName) {
  return FONT_FAMILY_MAP[fontName] || fontName || FONT_FAMILY_MAP['Arial.ttf']
}

let metricMeasureContext = null

function getMetricMeasureContext() {
  if (metricMeasureContext) {
    return metricMeasureContext
  }

  const canvas = document.createElement('canvas')
  metricMeasureContext = canvas.getContext('2d')
  return metricMeasureContext
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

export function measurePreviewText(text, fontSize, fontFamily) {
  if (!text) {
    return {
      width: 0,
      glyphHeight: 0,
      ascent: 0,
      descent: 0,
      boundsLeft: 0,
      boundsRight: 0,
    }
  }

  const context = getMetricMeasureContext()
  if (!context) {
    return {
      width: 0,
      glyphHeight: 0,
      ascent: 0,
      descent: 0,
      boundsLeft: 0,
      boundsRight: 0,
    }
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

export function getPreviewTextBaseline({
  top = 0,
  lineHeight,
  ascent,
  descent: _descent,
  glyphHeight,
}) {
  if (!glyphHeight) {
    return top + lineHeight
  }

  return top + ((lineHeight - glyphHeight) / 2 + ascent)
}

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
  const showUnitText = Boolean(showUnits && unitText)
  const unitsMeasure = showUnitText
    ? measurePreviewText(unitText, unitsFontSize, fontFamily)
    : {
        width: 0,
        glyphHeight: 0,
        ascent: 0,
        descent: 0,
        boundsLeft: 0,
        boundsRight: 0,
      }
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
    textGroupBottom - (valueLineHeight + valueMeasure.glyphHeight) / 2
  const unitsTop =
    textGroupBottom - (unitsLineHeight + unitsMeasure.glyphHeight) / 2
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
        ascent: valueMeasure.ascent,
        descent: valueMeasure.descent,
        glyphHeight: valueMeasure.glyphHeight,
      }),
      width: valueMeasure.width,
      lineHeight: valueLineHeight,
    },
    units: showUnitText
      ? {
          left: unitsLeft,
          top: unitsTop,
          baseline: getPreviewTextBaseline({
            top: unitsTop,
            lineHeight: unitsLineHeight,
            ascent: unitsMeasure.ascent,
            descent: unitsMeasure.descent,
            glyphHeight: unitsMeasure.glyphHeight,
          }),
          width: unitsMeasure.width,
          fontSize: unitsFontSize,
          lineHeight: unitsLineHeight,
        }
      : null,
    width,
    height: rowHeight,
    unitsFontSize,
  }
}

export function getWidgetOpacity(data, globalOpacity = 1) {
  return clamp((data?.opacity ?? 1) * globalOpacity, 0, 1)
}

export function getTextShadow(data) {
  const shadowStrength = Number(data?.shadow_strength) || 0
  const shadowDistance = Number(data?.shadow_distance) || 0
  const shadowColor = data?.shadow_color

  if (!shadowStrength || !shadowColor) return undefined

  return `${shadowDistance}px ${shadowDistance}px ${shadowStrength}px ${shadowColor}`
}

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

export function getCombinedTextShadow(data) {
  const outlineShadow = getTextOutlineShadow(data)
  const dropShadow = getTextShadow(data)

  if (outlineShadow && dropShadow) {
    return `${outlineShadow}, ${dropShadow}`
  }

  return outlineShadow || dropShadow || undefined
}

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
      units: unit === 'fahrenheit' ? 'F' : 'C',
    }
  }

  const numericValue = Number(value)
  if (unit === 'fahrenheit') {
    return {
      value: Math.round((numericValue * 9) / 5 + 32).toString(),
      units: 'F',
    }
  }

  return {
    value: Math.round(numericValue).toString(),
    units: 'C',
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

export function formatGradientValue(widget, value) {
  if (value === null || value === undefined) return '--'

  const decimals = widget.data.decimals ?? 0
  const numericValue = Number(value)
  const absoluteValue = Math.abs(numericValue).toFixed(decimals)
  const sign = numericValue > 0 ? '+' : numericValue < 0 ? '-' : ''
  const prefix = widget.data.show_sign === false ? '' : sign

  return `${prefix}${absoluteValue}`
}

export function buildGradientTrianglePath(value, width, height) {
  const normalized = clamp(Math.abs(Number(value) || 0) / 15, 0.12, 1)
  const centeredHeight = Math.max(height * 0.88, 4)
  const rise = Math.max((centeredHeight / 2) * normalized, 2)
  const centerY = height / 2

  if (Number(value) >= 0) {
    return `M 0 ${centerY} L ${width} ${centerY} L ${width} ${centerY - rise} Z`
  }

  return `M 0 ${centerY} L ${width} ${centerY} L ${width} ${centerY + rise} Z`
}
