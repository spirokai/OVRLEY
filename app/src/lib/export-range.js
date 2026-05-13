/**
 * Provides shared helpers for custom export range handling.
 */

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Clamped numeric value.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Converts a time string to seconds.
 *
 * @param {*} timeStr - Value in HH:MM:SS, MM:SS, or seconds form.
 * @returns {number} Parsed seconds value.
 */
export function timeToSeconds(timeStr) {
  if (timeStr === null || timeStr === undefined || timeStr === '') {
    return 0
  }

  const parts = String(timeStr)
    .split(':')
    .map((part) => Math.trunc(Number(part) || 0))
  if (parts.length === 3) {
    return parts[0] * 3600 + parts[1] * 60 + parts[2]
  }
  if (parts.length === 2) {
    return parts[0] * 60 + parts[1]
  }
  return parts[0] || 0
}

/**
 * Returns the activity duration in seconds.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @returns {number} Activity duration.
 */
export function getActivityDurationSeconds(activity) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const elapsedDuration = Number(elapsedSeries[elapsedSeries.length - 1]) || 0
  const trimmedDuration = Number(activity?.trim_end_seconds) || 0
  return Math.max(elapsedDuration, trimmedDuration, 0)
}

/**
 * Resolves the active custom export range for widget previews.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} exportRange - Range settings from store state.
 * @param {*} showFullActivity - Whether the widget should ignore the custom range.
 * @returns {object} Effective range window metadata.
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
 * Returns the normalized distance-progress span for the active export window.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} window - Effective export range window.
 * @returns {?object} Start/end distance progress and span metadata.
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
 * Normalizes a distance progress value within the active export window span.
 *
 * @param {*} value - Global distance progress value.
 * @param {*} distanceSpan - Effective distance-progress span metadata.
 * @returns {number} Progress rebased within the custom range.
 */
export function normalizeDistanceProgressToWindow(value, distanceSpan) {
  if (!distanceSpan || !Number.isFinite(value) || !(distanceSpan.span > 0)) {
    return 0
  }

  return clamp((Number(value) - distanceSpan.start) / distanceSpan.span, 0, 1)
}

/**
 * Returns the current normalized distance progress inside the export window.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} window - Effective export range window.
 * @param {*} elapsedSecond - Current preview second.
 * @returns {?number} Distance-based progress normalized to the selected range.
 */
export function getExportWindowDistanceProgressAtElapsed(activity, window, elapsedSecond) {
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
 * Returns the source course points used by route previews.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @returns {Array} Course point series.
 */
function getRouteSourcePoints(activity) {
  return Array.isArray(activity?.sample_course_points) ? activity.sample_course_points : []
}

/**
 * Returns the source elevation series used by elevation previews.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @returns {Array} Elevation series.
 */
function getElevationSourceValues(activity) {
  if (Array.isArray(activity?.sample_elevations) && activity.sample_elevations.some((value) => Number.isFinite(value))) {
    return activity.sample_elevations
  }

  return Array.isArray(activity?.elevation) ? activity.elevation : []
}

/**
 * Interpolates a numeric series at the target time.
 *
 * @param {*} elapsedSeries - Sample elapsed series.
 * @param {*} values - Numeric series aligned with elapsed samples.
 * @param {*} targetSecond - Requested elapsed second.
 * @returns {?number} Interpolated numeric value.
 */
function interpolateNumericSeries(elapsedSeries, values, targetSecond) {
  const validSamples = elapsedSeries.reduce((result, elapsed, index) => {
    const rawValue = values[index]
    if (rawValue === null || rawValue === undefined) {
      return result
    }

    const value = Number(rawValue)
    if (Number.isFinite(elapsed) && Number.isFinite(value)) {
      result.push([elapsed, value])
    }
    return result
  }, [])

  if (!validSamples.length) {
    return null
  }

  if (targetSecond <= validSamples[0][0]) {
    return validSamples[0][1]
  }

  const lastSample = validSamples[validSamples.length - 1]
  if (targetSecond >= lastSample[0]) {
    return lastSample[1]
  }

  for (let index = 1; index < validSamples.length; index += 1) {
    const [leftElapsed, leftValue] = validSamples[index - 1]
    const [rightElapsed, rightValue] = validSamples[index]
    if (rightElapsed < targetSecond) {
      continue
    }

    if (rightElapsed === leftElapsed) {
      return rightValue
    }

    const ratio = (targetSecond - leftElapsed) / (rightElapsed - leftElapsed)
    return leftValue + (rightValue - leftValue) * ratio
  }

  return lastSample[1]
}

/**
 * Interpolates a course point at the target time.
 *
 * @param {*} elapsedSeries - Sample elapsed series.
 * @param {*} coursePoints - Course point series aligned with elapsed samples.
 * @param {*} targetSecond - Requested elapsed second.
 * @returns {?number[]} Interpolated course point.
 */
function interpolateCoursePoint(elapsedSeries, coursePoints, targetSecond) {
  const latitudes = coursePoints.map((point) => (Array.isArray(point) ? point[0] : null))
  const longitudes = coursePoints.map((point) => (Array.isArray(point) ? point[1] : null))
  const latitude = interpolateNumericSeries(elapsedSeries, latitudes, targetSecond)
  const longitude = interpolateNumericSeries(elapsedSeries, longitudes, targetSecond)

  if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) {
    return null
  }

  return [latitude, longitude]
}

/**
 * Checks whether two course points are equal.
 *
 * @param {*} left - Left-hand point.
 * @param {*} right - Right-hand point.
 * @returns {boolean} Whether the points match.
 */
function coursePointsEqual(left, right) {
  return Array.isArray(left) && Array.isArray(right) && left[0] === right[0] && left[1] === right[1]
}

/**
 * Builds route samples scoped to the active export range.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} window - Effective export range window.
 * @returns {Array} Route samples for preview geometry.
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
 * Builds elevation series scoped to the active export range.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} window - Effective export range window.
 * @returns {object} Scoped elevation values and progress values.
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
