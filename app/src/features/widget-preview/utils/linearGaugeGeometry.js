/**
 * Gauge geometry utilities — pure functions for computing fill percentages,
 * rect layouts, value ranges, and label formatting for linear gauge widgets.
 *
 * These functions are used by both the SVG renderer (GaugeRenderer.jsx) and
 * the editor overlay (resize handles). They are intentionally stateless and
 * framework-agnostic to stay testable in isolation.
 *
 * @module linearGaugeGeometry
 */

/**
 * Computes the fill percentage of a value within a range.
 * Returns a value between 0 and 1, or 0 if the range is invalid.
 *
 * @param {number} value - The current value.
 * @param {number} min - The minimum of the range.
 * @param {number} max - The maximum of the range.
 * @returns {number} Fill fraction (0-1).
 */
export function getFillPercentage(value, min, max) {
  if (typeof value !== 'number' || !Number.isFinite(value) || max <= min) return 0
  return Math.min(1, Math.max(0, (value - min) / (max - min)))
}

/**
 * Computes the fill rectangle for a linear gauge bar, accounting for
 * orientation and border inset.
 *
 * @param {object} params
 * @param {number} params.x - Track left edge.
 * @param {number} params.y - Track top edge.
 * @param {number} params.width - Track width.
 * @param {number} params.height - Track height.
 * @param {number} params.fill - Fill fraction (0-1).
 * @param {string} [params.orientation='horizontal'] - 'horizontal' or 'vertical'.
 * @param {number} [params.borderThickness=0] - Border thickness to inset.
 * @returns {{ x: number, y: number, width: number, height: number }} Fill rect.
 */
export function getLinearFillRect({ x = 0, y = 0, width, height, fill, orientation = 'horizontal', borderThickness = 0 }) {
  const fill01 = Math.min(1, Math.max(0, fill || 0))
  const inset = Math.max(0, borderThickness || 0)
  const innerX = x + inset
  const innerY = y + inset
  const innerWidth = Math.max(0, width - inset * 2)
  const innerHeight = Math.max(0, height - inset * 2)
  if (orientation === 'vertical') {
    const filledHeight = innerHeight * fill01
    return { x: innerX, y: innerY + innerHeight - filledHeight, width: innerWidth, height: filledHeight }
  }
  return { x: innerX, y: innerY, width: innerWidth * fill01, height: innerHeight }
}

/**
 * Computes the min/max range from a series of values.
 * Falls back to 0-100 if no finite values exist.
 *
 * @param {number[]} values - Array of numeric values.
 * @returns {{ min: number, max: number }} The value range.
 */
export function getLinearGaugeRange(values) {
  const finiteValues = (values || []).filter((value) => typeof value === 'number' && Number.isFinite(value))
  if (finiteValues.length === 0) return { min: 0, max: 100 }
  const min = Math.min(...finiteValues)
  const max = Math.max(...finiteValues)
  return max > min ? { min, max } : { min: 0, max: 100 }
}

/**
 * Computes the complete linear gauge layout from widget data.
 * Returns the value range, fill percentage, and track/fill rectangles.
 *
 * @param {object} params
 * @param {number} params.value - Current metric value.
 * @param {number[]} params.values - Full metric series (for range computation).
 * @param {number} params.width - Track width.
 * @param {number} params.height - Track height.
 * @param {string} [params.orientation='horizontal'] - Gauge orientation.
 * @param {number} [params.borderThickness=0] - Border inset.
 * @returns {{ min: number, max: number, fill: number, trackRect: object, fillRect: object }}
 */
export function getLinearGaugeLayout({ value, values, width, height, orientation = 'horizontal', borderThickness = 0 }) {
  const range = getLinearGaugeRange(values)
  const hasValue = typeof value === 'number' && Number.isFinite(value)
  const fill = hasValue ? getFillPercentage(value, range.min, range.max) : 0.5
  return {
    ...range,
    fill,
    trackRect: { x: 0, y: 0, width, height },
    fillRect: getLinearFillRect({ x: 0, y: 0, width, height, fill, orientation, borderThickness }),
  }
}

/**
 * Formats a gauge label value. Integers are formatted without decimals;
 * non-integers show one decimal place.
 *
 * @param {number} value - The value to format.
 * @returns {string} Formatted label string.
 */
export function formatLinearGaugeLabel(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return ''
  return Number.isInteger(value) ? `${value}` : value.toFixed(1)
}
