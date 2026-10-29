import { ALTITUDE_UNIT_SCALES, WIDGET_TYPE_DEFINITIONS } from './widget-catalog'

/**
 * @param {object|null} activity
 * @param {object} source Manifest altitude source policy.
 * @returns {number|null} First activity altitude in canonical meters.
 */
export function getStartingAltitude(activity, source) {
  for (const { field, requireValue } of source.series) {
    const series = activity?.[field]
    if (!Array.isArray(series) || series.length === 0) continue
    for (const value of series) {
      if (value !== null && value !== undefined) return value
    }
    if (!requireValue) return null
  }
  return null
}

/**
 * @param {number} value
 * @param {string} fromUnit
 * @param {string} toUnit
 * @returns {number}
 */
export function convertAltitudeValue(value, fromUnit, toUnit) {
  if (fromUnit === toUnit) return value
  return (value / ALTITUDE_UNIT_SCALES[fromUnit]) * ALTITUDE_UNIT_SCALES[toUnit]
}

/**
 * @param {number|null|undefined|string} value
 * @param {string} fromUnit
 * @param {string} toUnit
 * @returns {number|null|undefined|string}
 */
export function convertAltitudeInputValue(value, fromUnit, toUnit) {
  if (value === null || value === undefined || value === '') return value
  return Math.round(convertAltitudeValue(value, fromUnit, toUnit))
}

/**
 * @param {string} metricType
 * @param {number|null} startingAltitude
 * @param {string} currentUnit
 * @param {string} nextUnit
 * @returns {object}
 */
export function buildMetricUnitUpdate(metricType, startingAltitude, currentUnit, nextUnit) {
  if (WIDGET_TYPE_DEFINITIONS[metricType].altitudeSource === null) return { display_unit: nextUnit }
  return {
    display_unit: nextUnit,
    starting_altitude: convertAltitudeInputValue(startingAltitude, currentUnit, nextUnit),
  }
}
