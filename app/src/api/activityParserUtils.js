/**
 * Implements API helpers for activity parser utils.
 */

import { buildDistanceSeries, buildElapsedSeries, buildProgressSeries, insertIdleGapSamples } from './activityGapUtils'
import { buildMetricCoverage, deriveActivityMetricSeries } from './activityMetricSeries'

export const CORE_ACTIVITY_ATTRIBUTES = ['cadence', 'course', 'elevation', 'gradient', 'heartrate', 'power', 'speed', 'time', 'temperature']

const EXTENDED_ACTIVITY_ATTRIBUTES = [
  'air_pressure',
  'altitude',
  'distance',
  'g_force',
  'ground_contact_time',
  'heading',
  'left_right_balance',
  'pace',
  'stroke_rate',
  'stride_length',
  'torque',
  'vertical_oscillation',
  'vertical_speed',
]

const METRIC_UNITS = {
  air_pressure: 'bar',
  altitude: 'm',
  cadence: 'rpm',
  distance: 'm',
  elevation: 'm',
  g_force: 'g',
  gradient: 'percent',
  ground_contact_time: 'ms',
  heading: 'degrees',
  heartrate: 'bpm',
  left_right_balance: 'raw',
  pace: 'seconds_per_km',
  power: 'watts',
  speed: 'mps',
  stride_length: 'raw',
  stroke_rate: 'strokes_per_minute',
  temperature: 'celsius',
  torque: 'nm',
  vertical_oscillation: 'raw',
  vertical_speed: 'mps',
}

/**
 * Checks whether is finite number.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {boolean} Whether the condition is satisfied.
 */
function isFiniteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value)
}

/**
 * Handles round value.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} digits - Value for digits.
 * @returns {number} Result produced by the helper.
 */
function roundValue(value, digits = 6) {
  if (!isFiniteNumber(value)) return null
  const scale = 10 ** digits
  return Math.round(value * scale) / scale
}

/**
 * Handles safe number.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {number} Result produced by the helper.
 */
export function safeNumber(value) {
  if (value === null || value === undefined || value === '') {
    return null
  }

  if (typeof value === 'number') {
    return Number.isFinite(value) ? value : null
  }

  if (typeof value === 'string') {
    const normalized = value.trim()
    if (!normalized) return null
    const numeric = Number(normalized)
    return Number.isFinite(numeric) ? numeric : null
  }

  return null
}

/**
 * Handles safe timestamp.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {number} Result produced by the helper.
 */
export function safeTimestamp(value) {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  if (!Number.isFinite(date.getTime())) return null
  return date.toISOString()
}

/**
 * Handles haversine distance meters.
 *
 * @param {*} lat1 - Value for lat1.
 * @param {*} lon1 - Value for lon1.
 * @param {*} lat2 - Value for lat2.
 * @param {*} lon2 - Value for lon2.
 * @returns {*} Result produced by the helper.
 */
export function haversineDistanceMeters(lat1, lon1, lat2, lon2) {
  if (!isFiniteNumber(lat1) || !isFiniteNumber(lon1) || !isFiniteNumber(lat2) || !isFiniteNumber(lon2)) {
    return 0
  }

  const earthRadiusMeters = 6371000
  const lat1Rad = (lat1 * Math.PI) / 180
  const lat2Rad = (lat2 * Math.PI) / 180
  const latDeltaRad = ((lat2 - lat1) * Math.PI) / 180
  const lonDeltaRad = ((lon2 - lon1) * Math.PI) / 180

  const a = Math.sin(latDeltaRad / 2) ** 2 + Math.cos(lat1Rad) * Math.cos(lat2Rad) * Math.sin(lonDeltaRad / 2) ** 2

  const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a))
  return earthRadiusMeters * c
}

/**
 * Handles calculate bearing degrees.
 *
 * @param {*} fromPoint - Value for from point.
 * @param {*} toPoint - Value for to point.
 * @returns {*} Result produced by the helper.
 */
function calculateBearingDegrees(fromPoint, toPoint) {
  if (!fromPoint || !toPoint) return null

  const [fromLat, fromLon] = fromPoint
  const [toLat, toLon] = toPoint
  if (!isFiniteNumber(fromLat) || !isFiniteNumber(fromLon) || !isFiniteNumber(toLat) || !isFiniteNumber(toLon)) {
    return null
  }

  const fromLatRad = (fromLat * Math.PI) / 180
  const toLatRad = (toLat * Math.PI) / 180
  const lonDeltaRad = ((toLon - fromLon) * Math.PI) / 180
  const y = Math.sin(lonDeltaRad) * Math.cos(toLatRad)
  const x = Math.cos(fromLatRad) * Math.sin(toLatRad) - Math.sin(fromLatRad) * Math.cos(toLatRad) * Math.cos(lonDeltaRad)

  const bearing = (Math.atan2(y, x) * 180) / Math.PI
  return (bearing + 360) % 360
}

/**
 * Creates activity helpers.
 * @returns {object} Derived data structure for downstream use.
 */
function createActivityHelpers() {
  return {
    calculateBearingDegrees,
    haversineDistanceMeters,
    isFiniteNumber,
    roundValue,
    safeNumber,
    safeTimestamp,
  }
}

/**
 * Builds course series.
 *
 * @param {*} rawSamples - Raw activity samples from the source file.
 * @returns {*} Derived data structure for downstream use.
 */
function buildCourseSeries(rawSamples) {
  return rawSamples.map((sample) => [safeNumber(sample.latitude), safeNumber(sample.longitude)])
}

/**
 * Builds time series.
 *
 * @param {*} rawSamples - Raw activity samples from the source file.
 * @returns {*} Derived data structure for downstream use.
 */
function buildTimeSeries(rawSamples) {
  return rawSamples.map((sample) => safeTimestamp(sample.timestamp))
}

/**
 * Builds valid attributes.
 *
 * @param {*} metricSeriesMap - Metric series keyed by metric identifier.
 * @param {*} courseSeries - Value for course series.
 * @param {*} timeSeries - Timestamp series for the activity.
 * @returns {*} Derived data structure for downstream use.
 */
function buildValidAttributes(metricSeriesMap, courseSeries, timeSeries) {
  return CORE_ACTIVITY_ATTRIBUTES.filter((attribute) => {
    if (attribute === 'course') {
      return courseSeries.some(([latitude, longitude]) => isFiniteNumber(latitude) && isFiniteNumber(longitude))
    }

    if (attribute === 'time') {
      return timeSeries.some(Boolean)
    }

    return metricSeriesMap[attribute].series.some((value) => value !== null)
  })
}

/**
 * Builds extended attributes.
 *
 * @param {*} metricSeriesMap - Metric series keyed by metric identifier.
 * @returns {*} Derived data structure for downstream use.
 */
function buildExtendedAttributes(metricSeriesMap) {
  return EXTENDED_ACTIVITY_ATTRIBUTES.filter((attribute) => metricSeriesMap[attribute].series.some((value) => value !== null))
}

/**
 * Handles finalize parsed activity.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.fileName - Value for file name.
 * @param {*} options.fileFormat - Value for file format.
 * @param {*} options.metadata - Value for metadata.
 * @param {*} options.rawSamples - Raw activity samples from the source file.
 * @returns {object} Result produced by the helper.
 */
export function finalizeParsedActivity({ fileName, fileFormat, metadata = {}, rawSamples = [], options = {} }) {
  const helpers = createActivityHelpers()
  const useLegacyGpxDerivations = options.useLegacyGpxDerivations === true
  const { rawSamples: normalizedRawSamples, gapDebug } = insertIdleGapSamples(rawSamples, helpers)
  const timeSeries = buildTimeSeries(normalizedRawSamples)
  const courseSeries = buildCourseSeries(normalizedRawSamples)
  const directDistanceSeries = normalizedRawSamples.map((sample) => safeNumber(sample.distance))
  const distanceSeries = buildDistanceSeries(courseSeries, directDistanceSeries, helpers)
  const elapsedSeries = buildElapsedSeries(normalizedRawSamples, timeSeries, helpers)
  const elevationBaseSeries = normalizedRawSamples.map((sample) => safeNumber(sample.elevation))
  const { metricSeriesMap } = deriveActivityMetricSeries({
    courseSeries,
    distanceSeries,
    elevationBaseSeries,
    elapsedSeries,
    normalizedRawSamples,
    useLegacyGpxDerivations,
    helpers,
  })

  const validAttributes = buildValidAttributes(metricSeriesMap, courseSeries, timeSeries)
  const extendedAttributes = buildExtendedAttributes(metricSeriesMap)
  const durationSeconds = elapsedSeries[elapsedSeries.length - 1] ?? 0
  const totalDistanceMeters = distanceSeries[distanceSeries.length - 1] ?? 0
  const startTime = timeSeries.find(Boolean) ?? null
  const endTime = [...timeSeries].reverse().find(Boolean) ?? null
  const coverage = buildMetricCoverage(metricSeriesMap)
  const distanceProgressSeries = buildProgressSeries(distanceSeries, helpers)

  const parsedActivity = {
    file_name: fileName,
    file_format: fileFormat,
    metadata: {
      ...metadata,
      duration_seconds: roundValue(durationSeconds, 3) ?? 0,
      start_time: startTime,
      end_time: endTime,
      total_distance_m: roundValue(totalDistanceMeters, 3) ?? 0,
      sample_count: normalizedRawSamples.length,
      original_sample_count: rawSamples.length,
      inserted_idle_sample_count: gapDebug.inserted_sample_count,
    },
    metric_units: METRIC_UNITS,
    coverage,
    valid_attributes: validAttributes,
    extended_attributes: extendedAttributes,
    source_start_time: startTime,
    sample_elapsed_seconds: elapsedSeries,
    sample_distance_progress: distanceProgressSeries,
    frame_elapsed_seconds: [],
    frame_timestamps: [],
    frame_distance_progress: [],
    trim_start_seconds: 0,
    trim_end_seconds: roundValue(durationSeconds, 3) ?? 0,
    sample_course_points: courseSeries,
    sample_elevations: metricSeriesMap.elevation.series,
    course: courseSeries,
    elevation: metricSeriesMap.elevation.series,
    time: timeSeries,
    speed: metricSeriesMap.speed.series,
    heartrate: metricSeriesMap.heartrate.series,
    cadence: metricSeriesMap.cadence.series,
    power: metricSeriesMap.power.series,
    temperature: metricSeriesMap.temperature.series,
    gradient: metricSeriesMap.gradient.series,
    altitude: metricSeriesMap.altitude.series,
    air_pressure: metricSeriesMap.air_pressure.series,
    distance: metricSeriesMap.distance.series,
    g_force: metricSeriesMap.g_force.series,
    ground_contact_time: metricSeriesMap.ground_contact_time.series,
    heading: metricSeriesMap.heading.series,
    left_right_balance: metricSeriesMap.left_right_balance.series,
    pace: metricSeriesMap.pace.series,
    stroke_rate: metricSeriesMap.stroke_rate.series,
    stride_length: metricSeriesMap.stride_length.series,
    torque: metricSeriesMap.torque.series,
    vertical_oscillation: metricSeriesMap.vertical_oscillation.series,
    vertical_speed: metricSeriesMap.vertical_speed.series,
  }

  return {
    parsedActivity,
    debugPayload: {
      generated_at: new Date().toISOString(),
      file_name: fileName,
      file_format: fileFormat,
      idle_gap_fill: gapDebug,
      parsed_activity: parsedActivity,
    },
  }
}
