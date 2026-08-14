/**
 * Shared gauge boundary-label conversion and integer formatting.
 *
 * All gauge types (arc, corner, linear) use this module to convert raw
 * telemetry min/max values through the selected display unit and to round
 * them to the nearest integer, keeping labels consistent with the inner
 * value that already respects the chosen unit system.
 */

import { convertStandardMetricValue, getStandardMetricDisplayUnit } from '@/lib/widget/standard-metrics'

/**
 * Converts a raw telemetry boundary value through the selected display unit
 * and rounds it to an integer label string.
 *
 * @param {string} metricType - Metric type key (e.g. "speed", "distance").
 * @param {number} value - Raw telemetry value in native units.
 * @param {string|null|undefined} displayUnit - Configured display unit, or
 *   void to use the metric's default.
 * @returns {string} Integer label string (e.g. "128").
 */
export function formatGaugeBoundaryLabel(metricType, value, displayUnit) {
  const effectiveUnit = displayUnit ?? getStandardMetricDisplayUnit(metricType)
  const converted = convertStandardMetricValue(metricType, value, effectiveUnit)
  return String(Math.round(converted))
}

/**
 * Resolves the display unit to use for a gauge widget's boundary labels.
 *
 * @param {string} metricType - Metric type key.
 * @param {object} widgetData - Widget instance data (may contain display_unit).
 * @returns {string|null} The resolved display unit, or null.
 */
export function getGaugeBoundaryLabelUnit(metricType, widgetData = {}) {
  return widgetData.display_unit || getStandardMetricDisplayUnit(metricType)
}
