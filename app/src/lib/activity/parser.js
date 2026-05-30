/**
 * Activity parsing pipeline — orchestrates gap filling, series derivation,
 * and finalization of parsed activity data.
 */

import { buildDistanceSeries, buildElapsedSeries, buildProgressSeries, insertIdleGapSamples } from './gap-utils.js'
import { buildMetricCoverage, deriveActivityMetricSeries } from './metric-series.js'
import { calculateBearingDegrees, haversineDistanceMeters, isFiniteNumber, roundValue, safeNumber, safeTimestamp } from './parse-helpers.js'

const CORE_ACTIVITY_ATTRIBUTES = ['cadence', 'course', 'elevation', 'gradient', 'heartrate', 'power', 'speed', 'time', 'temperature']

const EXTENDED_ACTIVITY_ATTRIBUTES = [
  'air_pressure',
  'altitude',
  'core_temperature',
  'distance',
  'g_force',
  'gear_position',
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
  core_temperature: 'celsius',
  distance: 'm',
  elevation: 'm',
  g_force: 'g',
  gear_position: 'raw',
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
  const helpers = { calculateBearingDegrees, haversineDistanceMeters, isFiniteNumber, roundValue, safeNumber, safeTimestamp }
  const useLegacyGpxDerivations = options.useLegacyGpxDerivations === true
  const { rawSamples: normalizedRawSamples, gapDebug } = insertIdleGapSamples(rawSamples)
  const timeSeries = buildTimeSeries(normalizedRawSamples)
  const courseSeries = buildCourseSeries(normalizedRawSamples)
  const directDistanceSeries = normalizedRawSamples.map((sample) => safeNumber(sample.distance))
  const distanceSeries = buildDistanceSeries(courseSeries, directDistanceSeries)
  const elapsedSeries = buildElapsedSeries(normalizedRawSamples, timeSeries)
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
  const distanceProgressSeries = buildProgressSeries(distanceSeries)

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
    core_temperature: metricSeriesMap.core_temperature.series,
    power: metricSeriesMap.power.series,
    temperature: metricSeriesMap.temperature.series,
    gradient: metricSeriesMap.gradient.series,
    altitude: metricSeriesMap.altitude.series,
    air_pressure: metricSeriesMap.air_pressure.series,
    distance: metricSeriesMap.distance.series,
    g_force: metricSeriesMap.g_force.series,
    gear_position: metricSeriesMap.gear_position.series,
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
