/**
 * Format utilities — speed, temperature, time, and gradient value formatting
 * as well as gradient widget layout computation.
 */

import {
  METRIC_WIDGET_LINE_HEIGHT,
  GRADIENT_WIDGET_TRIANGLE_GAP_PX,
  MAX_GRADIENT_ABS_PERCENT,
  GRADIENT_ZERO_EPSILON,
} from '@/features/overlay-editor/data/overlayEditorConstants'
import {
  convertStandardMetricValue,
  getStandardMetricDefinition,
  getStandardMetricDisplayUnit,
  getStandardMetricUnitLabel,
  getStandardMetricUnitsMode,
} from '@/lib/widget/standard-metrics'
import { measurePreviewText, getPreviewTextBaseline } from '../../shared/textMeasurement'

/**
 * Formats a speed value into a human-readable string with unit label.
 *
 * @param {number|null|undefined} value - Speed value in meters per second.
 * @param {string} unit - Target unit system ('kmh', 'mph', 'kn', 'mps').
 * @returns {{ value: string, units: string }} Formatted speed string and unit label.
 */
function formatSpeed(value, unit) {
  const conversions = {
    kmh: { units: 'KM/H' },
    mph: { units: 'MPH' },
    kn: { units: 'KN' },
    mps: { units: 'M/S' },
  }
  const selection = conversions[unit]

  if (value === null || value === undefined) {
    return { value: '--', units: selection.units }
  }

  return {
    value: Math.round(convertStandardMetricValue('speed', value, unit)).toString(),
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
function formatTemperature(value, unit, decimals) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units: unit === 'fahrenheit' ? '\u00B0F' : '\u00B0C',
    }
  }

  const temp = convertStandardMetricValue('temperature', value, unit)
  const roundedValue = decimals > 0 ? temp.toFixed(decimals) : Math.round(temp).toString()

  return {
    value: roundedValue,
    units: unit === 'fahrenheit' ? '\u00B0F' : '\u00B0C',
  }
}

function formatPace(value, unit) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units: unit === 'min_per_mi' ? 'MIN/MI' : 'MIN/KM',
    }
  }

  const totalSeconds = convertStandardMetricValue('pace', value, unit)
  const roundedSeconds = Math.max(Math.round(totalSeconds), 0)

  return {
    value: `${Math.floor(roundedSeconds / 60)}:${String(roundedSeconds % 60).padStart(2, '0')}`,
    units: unit === 'min_per_mi' ? 'MIN/MI' : 'MIN/KM',
  }
}

/** Formats a finite number with fixed decimals while normalizing negative zero. */
export function formatFixedDecimal(value, decimals) {
  const precision = decimals > 0 ? decimals : 0
  const factor = 10 ** precision
  const rounded = (Math.sign(value) * Math.round(Math.abs(value) * factor)) / factor
  const roundedValue = precision > 0 ? rounded.toFixed(precision) : rounded.toString()
  return Number(roundedValue) === 0 ? (0).toFixed(precision) : roundedValue
}

function formatRoundedMetric(value, units, decimals) {
  if (value === null || value === undefined) {
    return {
      value: '--',
      units,
    }
  }

  return {
    value: formatFixedDecimal(value, decimals),
    units,
  }
}

function formatDistanceValue(value, unit, decimals, showUnits) {
  const units = showUnits ? unit : ''
  if (value === null || value === undefined) {
    return {
      value: '--',
      units,
    }
  }

  return {
    value: formatRoundedMetric(value, '', decimals).value,
    units,
  }
}

/**
 * Converts a raw standard-metric telemetry value through the selected display
 * unit. Handles all metric kinds including speed, temperature, and pace that
 * have custom conversion logic.
 *
 * @param {string} type - Metric type key (e.g. "speed", "distance").
 * @param {string|null} displayUnit - Target display unit.
 * @param {number} value - Raw telemetry value in native units.
 * @returns {number} Converted value.
 */
const BALANCE_FORMATS = {
  plain: { valueTemplate: (l, r) => `${l}/${r}`, placeholder: '--/--' },
  l_prefix: { valueTemplate: (l, r) => `L${l}/R${r}`, placeholder: '--/--' },
  percent_label: { valueTemplate: (l, r) => `${l}%/${r}%`, placeholder: '--/--' },
  l_suffix: { valueTemplate: (l, r) => `${l}L/${r}R`, placeholder: '--/--' },
}

export const BALANCE_FORMAT_OPTIONS = [
  { value: 'percent_label', label: '52%/48%' },
  { value: 'plain', label: '52/48' },
  { value: 'l_prefix', label: 'L52/R48' },
  { value: 'l_suffix', label: '52L/48R' },
]

function formatBalance(value, decimals, balanceFormat) {
  const fmt = BALANCE_FORMATS[balanceFormat]

  if (value === null || value === undefined) {
    return {
      value: fmt.placeholder,
      units: '',
    }
  }

  // FIT's missing-balance sentinel can decode as 127; show degenerate values as neutral.
  const leftValue = value >= 100 ? 50 : Math.min(Math.max(value, 0), 100)
  const rightValue = Math.min(Math.max(100 - leftValue, 0), 100)
  const leftText = decimals > 0 ? leftValue.toFixed(decimals) : Math.round(leftValue).toString()
  const rightText = decimals > 0 ? rightValue.toFixed(decimals) : Math.round(rightValue).toString()

  return {
    value: fmt.valueTemplate(leftText, rightText),
    units: '',
  }
}

function formatAperture(value) {
  if (value === null || value === undefined) {
    return { value: '--', units: '' }
  }

  if (value <= 0) {
    return { value: '--', units: '' }
  }

  return { value: `F/${value.toFixed(1)}`, units: '' }
}

function formatShutterSpeed(value) {
  if (value === null || value === undefined) {
    return { value: '--', units: '' }
  }

  if (value <= 0) {
    return { value: '--', units: '' }
  }

  const denominator = Math.round(1 / value)
  return { value: `1/${denominator}`, units: '' }
}

function formatEv(value, decimals) {
  if (value === null || value === undefined) {
    return { value: '--', units: '' }
  }

  const abs = Math.abs(value)
  const effectiveDecimals = abs === 0 ? Math.max(decimals, 1) : decimals
  const formatted = abs.toFixed(effectiveDecimals)
  const prefix = value > 0 ? '+' : value < 0 ? '-' : ''

  return { value: `${prefix}${formatted}`, units: '' }
}

function formatGearPosition(value, units) {
  if (value === null || value === undefined) return { value: '--', units }
  if (value === '0') return { value: 'N', units }
  return { value, units }
}

function formatCoordinatePlaceholder(coordinateFormat) {
  if (coordinateFormat === 'dms') return '--°--′--″'
  if (coordinateFormat === 'ddm') return '--°--.---′'
  throw new Error(`Unknown GPS coordinate format: ${coordinateFormat}`)
}

function formatDmsCoordinate(absolute) {
  let degrees = Math.floor(absolute)
  const minutesTotal = (absolute - degrees) * 60
  let minutes = Math.floor(minutesTotal)
  let seconds = Math.round((minutesTotal - minutes) * 60)
  if (seconds === 60) {
    seconds = 0
    minutes += 1
  }
  if (minutes === 60) {
    minutes = 0
    degrees += 1
  }
  return `${degrees}\u00B0${String(minutes).padStart(2, '0')}\u2032${String(seconds).padStart(2, '0')}\u2033`
}

function formatDdmCoordinate(absolute) {
  let degrees = Math.floor(absolute)
  let decimalMinutes = (absolute - degrees) * 60
  if (decimalMinutes >= 59.9995) {
    decimalMinutes = 0
    degrees += 1
  }
  return `${degrees}\u00B0${decimalMinutes.toFixed(3).padStart(6, '0')}\u2032`
}

function formatCoordinateLine(coordinate, isLatitude, coordinateFormat, directionColor) {
  if (coordinate === null || coordinate === undefined) {
    return { direction: '', valueText: formatCoordinatePlaceholder(coordinateFormat), directionColor }
  }

  const direction = isLatitude ? (coordinate < 0 ? 'S' : 'N') : coordinate < 0 ? 'W' : 'E'
  const valueFormatter = { dms: formatDmsCoordinate, ddm: formatDdmCoordinate }[coordinateFormat]
  return {
    direction,
    valueText: valueFormatter(Math.abs(coordinate)),
    directionColor,
  }
}

/**
 * Formats decimal latitude/longitude values as DMS or DDM text.
 *
 * @param {[number|null, number|null]} value - Latitude/longitude pair.
 * @param {'latitude'|'longitude'|'both'} displayUnit - Coordinate selection.
 * @param {'dms'|'ddm'} coordinateFormat - Coordinate notation.
 * @param {string} unitColor - Direction-letter color consumed by the renderer.
 * @returns {{type: 'coordinates', lines: Array<{direction: string, valueText: string, directionColor: string}>}} Coordinate display lines.
 */
export function formatCoordinates(value, displayUnit, coordinateFormat, unitColor) {
  const [latitude, longitude] = value
  const latitudeLine = formatCoordinateLine(latitude, true, coordinateFormat, unitColor)
  const longitudeLine = formatCoordinateLine(longitude, false, coordinateFormat, unitColor)

  if (displayUnit === 'latitude') return { type: 'coordinates', lines: [latitudeLine] }
  if (displayUnit === 'longitude') return { type: 'coordinates', lines: [longitudeLine] }
  if (displayUnit === 'both') return { type: 'coordinates', lines: [latitudeLine, longitudeLine] }
  throw new Error(`Unknown GPS coordinate display unit: ${displayUnit}`)
}

export function formatStandardMetricDisplay(type, value, widgetData) {
  const definition = getStandardMetricDefinition(type)
  if (!definition) throw new Error(`Unknown standard metric type: ${type}`)

  const displayUnit = getStandardMetricDisplayUnit(type, widgetData)
  const unitLabel = getStandardMetricUnitLabel(type, displayUnit)
  const unitsMode = getStandardMetricUnitsMode(type)
  const effectiveUnitLabel = unitsMode === 'hidden' ? '' : unitLabel
  const showUnits = widgetData.show_units

  if (definition.formatter === 'speed') {
    return formatSpeed(value, displayUnit)
  }

  if (definition.formatter === 'temperature') {
    return formatTemperature(value, displayUnit, widgetData.decimals)
  }

  if (definition.formatter === 'pace') {
    return formatPace(value, displayUnit)
  }

  if (definition.formatter === 'balance') {
    return formatBalance(value, widgetData.decimals, widgetData.balance_format)
  }

  if (definition.formatter === 'aperture') {
    return formatAperture(value)
  }

  if (definition.formatter === 'shutter') {
    return formatShutterSpeed(value)
  }

  if (definition.formatter === 'ev') {
    return formatEv(value, widgetData.decimals)
  }

  if (definition.formatter === 'gear') {
    return formatGearPosition(value, effectiveUnitLabel)
  }

  if (definition.formatter === 'coordinates') {
    return formatCoordinates(value, displayUnit, widgetData.coordinate_format, widgetData.unit_color)
  }

  if (definition.formatter === 'distance') {
    return formatDistanceValue(
      value === null || value === undefined ? value : convertStandardMetricValue(type, value, displayUnit),
      effectiveUnitLabel,
      widgetData.decimals,
      showUnits,
    )
  }

  return formatRoundedMetric(
    value === null || value === undefined ? value : convertStandardMetricValue(type, value, displayUnit),
    effectiveUnitLabel,
    widgetData.decimals,
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
 * @param {string|null|undefined} timezone - Optional IANA timezone from activity metadata.
 * @returns {string} Formatted time/date string.
 */
export function formatTimeValue(format, timestamp, timezone) {
  // Early return — missing or invalid timestamps show a placeholder
  if (!timestamp) return '--:--'

  const date = new Date(timestamp)
  if (!Number.isFinite(date.getTime())) return '--:--'

  // Parsed activity timestamps are canonical UTC values. An absent activity
  // timezone must remain deterministic and must never resolve through the
  // user's computer timezone.
  const timezoneOptions = { timeZone: timezone || 'UTC' }
  const dateTimeParts = Object.fromEntries(
    new Intl.DateTimeFormat('en-GB', {
      ...timezoneOptions,
      calendar: 'gregory',
      numberingSystem: 'latn',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    })
      .formatToParts(date)
      .map(({ type, value }) => [type, value]),
  )

  // Extract all date/time components for format string composition. Intl applies
  // the IANA zone's historical and daylight-saving rules when one is supplied.
  const day = dateTimeParts.day
  const month = dateTimeParts.month
  const year = dateTimeParts.year
  const shortMonth = new Intl.DateTimeFormat('en-US', { ...timezoneOptions, month: 'short' }).format(date).toUpperCase()
  const longMonth = new Intl.DateTimeFormat('en-US', { ...timezoneOptions, month: 'long' }).format(date).toUpperCase()
  const hour24 = dateTimeParts.hour
  const hour12Raw = Number(hour24) % 12 || 12
  const hour12 = padNumber(hour12Raw)
  const minutes = dateTimeParts.minute
  const seconds = dateTimeParts.second
  const suffix = Number(hour24) >= 12 ? 'PM' : 'AM'

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
    'time-24s': `${hour24}:${minutes}:${seconds}`,
    'time-12': `${hour12}:${minutes} ${suffix}`,
    'time-12s': `${hour12}:${minutes}:${seconds} ${suffix}`,
    'date-time-24': `${day}-${month}-${year} ${hour24}:${minutes}`,
    'date-time-24s': `${day}-${month}-${year} ${hour24}:${minutes}:${seconds}`,
    'date-time-12': `${day}-${month}-${year} ${hour12}:${minutes} ${suffix}`,
    'date-time-12s': `${day}-${month}-${year} ${hour12}:${minutes}:${seconds} ${suffix}`,
    'date-mmm-time-24': `${day} ${shortMonth} ${hour24}:${minutes}`,
    'date-mmm-time-12': `${day} ${shortMonth} ${hour12}:${minutes} ${suffix}`,
    'date-mmmm-time-24': `${day} ${longMonth} ${hour24}:${minutes}`,
    'date-mmmm-time-12': `${day} ${longMonth} ${hour12}:${minutes} ${suffix}`,
  }

  const formatted = formatMap[format]
  if (!formatted) throw new Error(`Unknown time format: ${format}`)
  return formatted
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

  const absoluteValue = Math.abs(value).toFixed(widget.data.decimals)
  const sign = value > 0 ? '+' : value < 0 ? '-' : ''
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
function getGradientTriangleHeight(value, width) {
  const magnitude = Math.min(Math.abs(value), MAX_GRADIENT_ABS_PERCENT)
  if (magnitude <= GRADIENT_ZERO_EPSILON) {
    return 0
  }

  const fullAngleRadians = (magnitude * Math.PI) / 180
  return width * Math.tan(fullAngleRadians)
}

/**
 * Checks whether a gradient value is effectively zero (within GRADIENT_ZERO_EPSILON).
 *
 * @param {number} value - Gradient value.
 * @returns {boolean} True if the value is zero, non-finite, or within epsilon.
 */
function isGradientZero(value) {
  return Math.abs(value) <= GRADIENT_ZERO_EPSILON
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
  const scaledValueOffset = valueOffset / (scale ?? 1)

  // Triangle dimensions — compute the max possible height and actual height based on gradient magnitude
  const maxTriangleHeight = showTriangle ? getGradientTriangleHeight(MAX_GRADIENT_ABS_PERCENT, triangleWidth) : 0
  const triangleHeight = showTriangle ? getGradientTriangleHeight(gradientValue, triangleWidth) : 0
  const contentWidth = Math.max(valueMeasure.width, showTriangle ? triangleWidth : 0)
  const indicatorTop = valueLineHeight + GRADIENT_WIDGET_TRIANGLE_GAP_PX
  const zeroBaseline = indicatorTop + maxTriangleHeight
  const anchoredValueTop = -scaledValueOffset
  const indicatorHeight = showTriangle ? maxTriangleHeight * 2 : 0

  // Content bounding box — compute the raw vertical extent of value + indicator, then calculate baseline
  const rawMinY = Math.min(0, anchoredValueTop)
  const rawMaxY = Math.max(anchoredValueTop + valueLineHeight, showTriangle ? indicatorTop + indicatorHeight : anchoredValueTop + valueLineHeight)
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
    triangle: showTriangle
      ? {
          left: (contentWidth - triangleWidth) / 2,
          top: indicatorTop - yOffset,
          width: triangleWidth,
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
  if (width <= 0 || height <= 0 || Math.abs(value) <= GRADIENT_ZERO_EPSILON) {
    return ''
  }

  if (value > 0) {
    return `M 0 0 L ${width} 0 L ${width} ${-height} Z`
  }

  return `M 0 0 L ${width} 0 L ${width} ${height} Z`
}
