import { convertStandardMetricValue } from './standard-metrics'

/**
 * @param {object|null} activity
 * @returns {Array<unknown>|undefined}
 */
export function getPreferredElevationSeries(activity) {
  const barometricSeries = activity?.barometric_altitude
  if (Array.isArray(barometricSeries) && barometricSeries.some((value) => value !== null && value !== undefined)) return barometricSeries
  return activity?.elevation
}

/**
 * @param {object|null} activity
 * @returns {Array<unknown>|undefined}
 */
export function getElevationProfileSeries(activity) {
  return activity?.sample_elevations?.length ? activity.sample_elevations : activity?.elevation
}

/**
 * @param {Array<unknown>} series
 * @returns {*|null}
 */
export function getFirstAltitudeValue(series) {
  return series.find((value) => value !== null && value !== undefined) ?? null
}

/**
 * @param {number} value
 * @param {string} fromUnit
 * @param {string} toUnit
 * @returns {number}
 */
export function convertAltitudeValue(value, fromUnit, toUnit) {
  if (fromUnit === toUnit) return value
  const fromScale = convertStandardMetricValue('altitude', 1, fromUnit)
  const toScale = convertStandardMetricValue('altitude', 1, toUnit)
  return (value / fromScale) * toScale
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
  if (metricType !== 'altitude') return { display_unit: nextUnit }
  return {
    display_unit: nextUnit,
    starting_altitude: convertAltitudeInputValue(startingAltitude, currentUnit, nextUnit),
  }
}

/**
 * @param {Array<unknown>} series
 * @param {number|null} startingAltitude
 * @param {string} unit
 * @returns {number}
 */
export function getAltitudeCorrectionMeters(series, startingAltitude, unit) {
  if (startingAltitude === null || startingAltitude === undefined || startingAltitude === '') return 0
  const firstValue = getFirstAltitudeValue(series)
  if (firstValue === null) return 0
  return convertAltitudeValue(startingAltitude, unit, 'm') - firstValue
}

/**
 * @param {number|null|undefined} value
 * @param {number} offset
 * @returns {number|null|undefined}
 */
export function applyAltitudeOffset(value, offset) {
  if (value === null || value === undefined) return value
  return value + offset
}
