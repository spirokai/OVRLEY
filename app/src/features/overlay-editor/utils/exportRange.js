/**
 * Provides shared helpers for custom export range handling.
 */

import { clamp } from '@/lib/utils'
import { interpolateCoursePoint, interpolateNumericSeries, coursePointsEqual } from '@/lib/interpolation'

/**
 * Converts a time string (HH:MM:SS, MM:SS, or plain seconds) to seconds.
 * Empty/null input returns 0.
 *
 * @param {string|null|undefined} timeStr - Time string in HH:MM:SS or MM:SS format.
 * @returns {number} Total seconds.
 */
export function timeToSeconds(timeStr) {
  if (timeStr === null || timeStr === undefined || timeStr === '') {
    return 0
  }

  const str = String(timeStr).trim()
  if (str === '') return 0

  const isNegative = str.startsWith('-')
  const absStr = isNegative ? str.substring(1) : str

  if (absStr.includes(':')) {
    const parts = absStr.split(':')
    let seconds = 0
    if (parts.length === 3) {
      seconds = parseInt(parts[0], 10) * 3600 + parseInt(parts[1], 10) * 60 + parseFloat(parts[2])
    } else if (parts.length === 2) {
      seconds = parseInt(parts[0], 10) * 60 + parseFloat(parts[1])
    }
    return isNegative ? -seconds : seconds
  }

  const parsed = parseFloat(str)
  return isNaN(parsed) ? 0 : parsed
}

/**
 * Returns the activity duration in seconds — the max of the last elapsed
 * sample and the trim_end_seconds.
 *
 * @param {object|null} activity - Parsed activity data.
 * @returns {number} Duration in seconds.
 */
export function getActivityDurationSeconds(activity) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const elapsedDuration = Number(elapsedSeries[elapsedSeries.length - 1]) || 0
  const trimmedDuration = Number(activity?.trim_end_seconds) || 0
  return Math.max(elapsedDuration, trimmedDuration, 0)
}

/**
 * Resolves the effective export range window for widget scoping.
 * If showFullActivity is true or exportRange is not 'custom', returns
 * an inactive window covering the full activity duration.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {object|null} exportRange - Export range config from store.
 * @param {boolean} [showFullActivity=false] - Whether to ignore the custom range.
 * @returns {{ active: boolean, duration: number, start: number, end: number }}
 *   Effective range window metadata.
 */
export function resolveExportRangeWindow(activity, exportRange, showFullActivity = false) {
  const duration = getActivityDurationSeconds(activity)

  if (showFullActivity || exportRange?.type !== 'custom') {
    return {
      active: false,
      duration,
      start: 0,
      end: duration,
    }
  }

  const start = clamp(timeToSeconds(exportRange?.fromTime), 0, duration)
  const end = clamp(timeToSeconds(exportRange?.toTime), 0, duration)

  if (!(end > start)) {
    return {
      active: false,
      duration,
      start: 0,
      end: duration,
    }
  }

  return {
    active: true,
    duration,
    start,
    end,
  }
}

/**
 * Interpolates the distance-progress values at the start and end of the
 * export window, returning the span metadata needed for rebasing progress.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @returns {{ start: number, end: number, span: number }|null} Distance span metadata.
 */
export function getExportWindowDistanceSpan(activity, window) {
  if (!window?.active) {
    return null
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  if (!elapsedSeries.length || !distanceProgress.length) {
    return null
  }

  const startProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, window.start)
  const endProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, window.end)

  if (!Number.isFinite(startProgress) || !Number.isFinite(endProgress)) {
    return null
  }

  const span = endProgress - startProgress
  if (!(span > 0)) {
    return null
  }

  return {
    start: clamp(startProgress, 0, 1),
    end: clamp(endProgress, 0, 1),
    span,
  }
}

/**
 * Rebases a global distance progress value to the export window's span,
 * returning 0 at window start and 1 at window end.
 *
 * @param {number} value - Global distance progress (0–1).
 * @param {{ start: number, span: number }|null} distanceSpan - Distance span metadata.
 * @returns {number} Window-relative progress (0–1).
 */
export function normalizeDistanceProgressToWindow(value, distanceSpan) {
  if (!distanceSpan || !Number.isFinite(value) || !(distanceSpan.span > 0)) {
    return 0
  }

  return clamp((Number(value) - distanceSpan.start) / distanceSpan.span, 0, 1)
}

/**
 * Returns the current distance-based playback progress normalized within
 * the active export window at the given elapsed second.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @param {number} elapsedSecond - Current elapsed second.
 * @returns {number|null} Window-normalized progress (0–1) or null if not applicable.
 */
export function getWindowProgressAtTime(activity, window, elapsedSecond) {
  const distanceSpan = getExportWindowDistanceSpan(activity, window)
  if (!distanceSpan) {
    return null
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []
  const currentProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, elapsedSecond)

  if (!Number.isFinite(currentProgress)) {
    return null
  }

  return normalizeDistanceProgressToWindow(currentProgress, distanceSpan)
}

/**
 * Extracts the sample_course_points array from activity data.
 *
 * @param {object|null} activity - Parsed activity data.
 * @returns {Array} Course point series.
 */
function getRouteSourcePoints(activity) {
  return Array.isArray(activity?.sample_course_points) ? activity.sample_course_points : []
}

/**
 * Extracts the elevation series from activity data — prefers sample_elevations
 * (from processed course points), falls back to raw elevation series.
 *
 * @param {object|null} activity - Parsed activity data.
 * @returns {number[]} Elevation value series.
 */
function getElevationSourceValues(activity) {
  if (Array.isArray(activity?.sample_elevations) && activity.sample_elevations.some((value) => Number.isFinite(value))) {
    return activity.sample_elevations
  }

  return Array.isArray(activity?.elevation) ? activity.elevation : []
}

/**
 * Builds route (course point) samples trimmed to the active export window.
 * Includes interpolated start/end points and rebased progress values.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @returns {Array<{ point: number[], progress: number|null }>} Scoped route samples.
 */
export function buildScopedRouteSamples(activity, window) {
  const coursePoints = getRouteSourcePoints(activity)
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  if (!window?.active || !elapsedSeries.length) {
    return coursePoints.reduce((result, point, index) => {
      if (!Array.isArray(point) || !Number.isFinite(point[0]) || !Number.isFinite(point[1])) {
        return result
      }

      result.push({
        point,
        progress: clamp(Number(distanceProgress[index]) || 0, 0, 1),
      })
      return result
    }, [])
  }

  const scopedSamples = []
  const distanceSpan = getExportWindowDistanceSpan(activity, window)
  const startPoint = interpolateCoursePoint(elapsedSeries, coursePoints, window.start)

  if (startPoint) {
    scopedSamples.push({
      point: startPoint,
      progress: distanceSpan ? 0 : null,
    })
  }

  for (let index = 0; index < elapsedSeries.length; index += 1) {
    const elapsed = Number(elapsedSeries[index])
    const point = coursePoints[index]
    if (
      !Number.isFinite(elapsed) ||
      elapsed <= window.start ||
      elapsed >= window.end ||
      !Array.isArray(point) ||
      !Number.isFinite(point[0]) ||
      !Number.isFinite(point[1])
    ) {
      continue
    }

    scopedSamples.push({
      point,
      progress: normalizeDistanceProgressToWindow(Number(distanceProgress[index]), distanceSpan),
    })
  }

  const endPoint = interpolateCoursePoint(elapsedSeries, coursePoints, window.end)
  if (endPoint) {
    const lastPoint = scopedSamples[scopedSamples.length - 1]?.point
    const lastProgress = scopedSamples[scopedSamples.length - 1]?.progress

    if (!coursePointsEqual(lastPoint, endPoint) || lastProgress !== 1) {
      scopedSamples.push({
        point: endPoint,
        progress: 1,
      })
    }
  }

  return scopedSamples
}

/**
 * Builds the elevation series trimmed to the active export window.
 * Includes interpolated start/end values and rebased distance progress.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @returns {{ values: number[], progressValues: (number|null)[] }} Scoped elevation series.
 */
export function buildScopedElevationSeries(activity, window) {
  const values = getElevationSourceValues(activity)
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []

  if (!window?.active || !elapsedSeries.length) {
    return {
      values,
      progressValues: Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : [],
    }
  }

  const scopedValues = []
  const scopedProgressValues = []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []
  const distanceSpan = getExportWindowDistanceSpan(activity, window)
  const startValue = interpolateNumericSeries(elapsedSeries, values, window.start)

  if (Number.isFinite(startValue)) {
    scopedValues.push(startValue)
    scopedProgressValues.push(distanceSpan ? 0 : null)
  }

  for (let index = 0; index < elapsedSeries.length; index += 1) {
    const elapsed = Number(elapsedSeries[index])
    const rawValue = values[index]
    const value = rawValue === null || rawValue === undefined ? null : Number(rawValue)
    if (!Number.isFinite(elapsed) || elapsed <= window.start || elapsed >= window.end || !Number.isFinite(value)) {
      continue
    }

    scopedValues.push(value)
    scopedProgressValues.push(normalizeDistanceProgressToWindow(Number(distanceProgress[index]), distanceSpan))
  }

  const endValue = interpolateNumericSeries(elapsedSeries, values, window.end)
  if (Number.isFinite(endValue)) {
    if (scopedValues[scopedValues.length - 1] !== endValue || scopedProgressValues[scopedProgressValues.length - 1] !== 1) {
      scopedValues.push(endValue)
      scopedProgressValues.push(1)
    }
  }

  return {
    values: scopedValues,
    progressValues: scopedProgressValues,
  }
}
