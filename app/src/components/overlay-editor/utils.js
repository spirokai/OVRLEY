/**
 * Provides overlay editor helpers for utils.
 */

import { DEFAULT_ACTIVITY_PREVIEW } from './constants'

export {
  areaToSvg,
  buildWidgetTransform,
  getCompletedIndex,
  normalizeElevationGeometry,
  getPointAtMetricProgress,
  getPointAtProgress,
  normalizeRouteGeometry,
  normalizeRoutePoints,
  pointsToSvg,
} from './geometryUtils'
export {
  buildGradientTrianglePath,
  formatGradientValue,
  getGradientTriangleHeight,
  getGradientWidgetLayout,
  formatSpeed,
  formatTemperature,
  formatTimeValue,
  getCombinedTextShadow,
  getMetricWidgetLayout,
  getPreviewFontFamily,
  getPreviewTextBaseline,
  getTextOutlineShadow,
  getTextShadow,
  getWidgetOpacity,
  METRIC_WIDGET_LINE_HEIGHT,
  METRIC_WIDGET_OUTER_GAP_PX,
  METRIC_WIDGET_UNITS_GAP_PX,
  measurePreviewText,
} from './metricTextUtils'

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Returns elapsed series.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @returns {*} Requested value or structure.
 */
export function getElapsedSeries(activity) {
  const frameElapsedSeries = activity?.frame_elapsed_seconds
  if (Array.isArray(frameElapsedSeries) && frameElapsedSeries.length) {
    return frameElapsedSeries
  }

  return Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
}

/**
 * Returns scene size.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {object} Requested value or structure.
 */
export function getSceneSize(config) {
  return {
    width: config?.scene?.width || 1920,
    height: config?.scene?.height || 1080,
  }
}

/**
 * Finds closest sample index.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} selectedSecond - Value for selected second.
 * @returns {*} Requested value or structure.
 */
export function findClosestSampleIndex(activity, selectedSecond) {
  const elapsedSeries = getElapsedSeries(activity)
  if (!elapsedSeries.length) return 0

  let low = 0
  let high = elapsedSeries.length - 1
  let result = 0

  while (low <= high) {
    const middle = Math.floor((low + high) / 2)
    const candidate = Number(elapsedSeries[middle]) || 0

    if (candidate <= selectedSecond) {
      result = middle
      low = middle + 1
    } else {
      high = middle - 1
    }
  }

  return result
}

/**
 * Returns sample value.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} key - Lookup key for the requested value.
 * @param {*} sampleIndex - Sample index within the activity series.
 * @returns {*} Requested value or structure.
 */
export function getSampleValue(activity, key, sampleIndex) {
  const series = activity?.[key]
  if (!Array.isArray(series)) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  return series[sampleIndex] ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

/**
 * Returns interpolated series value.
 *
 * @param {*} xValues - Series of x-axis values used for interpolation.
 * @param {*} yValues - Series of y-axis values used for interpolation.
 * @param {*} targetX - Value for target x.
 * @returns {*} Requested value or structure.
 */
export function getInterpolatedSeriesValue(xValues, yValues, targetX) {
  if (!Array.isArray(xValues) || !Array.isArray(yValues) || !xValues.length) {
    return null
  }

  const safeTargetX = Number(targetX)
  if (!Number.isFinite(safeTargetX)) {
    return null
  }

  let firstValidIndex = -1
  let lastValidIndex = -1

  for (let index = 0; index < xValues.length; index += 1) {
    if (Number.isFinite(xValues[index]) && Number.isFinite(yValues[index])) {
      firstValidIndex = index
      break
    }
  }

  if (firstValidIndex === -1) {
    return null
  }

  for (let index = yValues.length - 1; index >= 0; index -= 1) {
    if (Number.isFinite(xValues[index]) && Number.isFinite(yValues[index])) {
      lastValidIndex = index
      break
    }
  }

  if (safeTargetX <= xValues[firstValidIndex]) {
    return Number(yValues[firstValidIndex])
  }

  if (safeTargetX >= xValues[lastValidIndex]) {
    return Number(yValues[lastValidIndex])
  }

  let leftIndex = firstValidIndex
  let rightIndex = firstValidIndex

  for (let index = firstValidIndex + 1; index <= lastValidIndex; index += 1) {
    const nextX = Number(xValues[index])
    const nextY = Number(yValues[index])
    if (!Number.isFinite(nextX) || !Number.isFinite(nextY)) {
      continue
    }

    if (nextX >= safeTargetX) {
      rightIndex = index
      break
    }

    leftIndex = index
  }

  const leftX = Number(xValues[leftIndex])
  const rightX = Number(xValues[rightIndex])
  const leftY = Number(yValues[leftIndex])
  const rightY = Number(yValues[rightIndex])

  if (
    !Number.isFinite(leftX) ||
    !Number.isFinite(rightX) ||
    !Number.isFinite(leftY) ||
    !Number.isFinite(rightY)
  ) {
    return null
  }

  if (rightIndex === leftIndex || rightX === leftX) {
    return leftY
  }

  const ratio = (safeTargetX - leftX) / (rightX - leftX)
  return leftY + (rightY - leftY) * ratio
}

/**
 * Returns interpolated activity value.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} key - Lookup key for the requested value.
 * @param {*} elapsedSecond - Elapsed playback time in seconds.
 * @returns {*} Requested value or structure.
 */
export function getInterpolatedActivityValue(activity, key, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const series = activity?.[key]

  if (!Array.isArray(series) || !elapsedSeries.length) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  const interpolatedValue = getInterpolatedSeriesValue(
    elapsedSeries,
    series,
    elapsedSecond,
  )

  return interpolatedValue ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

/**
 * Returns interpolated time value.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} elapsedSecond - Elapsed playback time in seconds.
 * @returns {*} Requested value or structure.
 */
export function getInterpolatedTimeValue(activity, elapsedSecond) {
  const sourceStartTimeMs = Date.parse(activity?.source_start_time || '')
  if (Number.isFinite(sourceStartTimeMs)) {
    return new Date(
      sourceStartTimeMs + Math.max(elapsedSecond, 0) * 1000,
    ).toISOString()
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const timeSeries = Array.isArray(activity?.time) ? activity.time : []
  const numericTimeSeries = timeSeries.map((value) => {
    const parsed = Date.parse(value || '')
    return Number.isFinite(parsed) ? parsed : null
  })
  const interpolatedTimeMs = getInterpolatedSeriesValue(
    elapsedSeries,
    numericTimeSeries,
    elapsedSecond,
  )

  return Number.isFinite(interpolatedTimeMs)
    ? new Date(interpolatedTimeMs).toISOString()
    : DEFAULT_ACTIVITY_PREVIEW.time
}

/**
 * Returns distance progress.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} sampleIndex - Sample index within the activity series.
 * @returns {*} Requested value or structure.
 */
export function getDistanceProgress(activity, sampleIndex) {
  const distanceProgressSeries =
    activity?.frame_distance_progress?.length > 0
      ? activity.frame_distance_progress
      : activity?.sample_distance_progress?.length > 0
        ? activity.sample_distance_progress
        : null

  if (distanceProgressSeries) {
    const progressValue =
      distanceProgressSeries[
        clamp(sampleIndex, 0, distanceProgressSeries.length - 1)
      ]

    return clamp(Number(progressValue) || 0, 0, 1)
  }

  const elapsedSeries = getElapsedSeries(activity)
  if (elapsedSeries.length <= 1) {
    return 0
  }

  return clamp(sampleIndex / (elapsedSeries.length - 1), 0, 1)
}

/**
 * Returns distance progress at elapsed.
 *
 * @param {*} activity - Parsed activity data for previews or rendering.
 * @param {*} elapsedSecond - Elapsed playback time in seconds.
 * @returns {*} Requested value or structure.
 */
export function getDistanceProgressAtElapsed(activity, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds)
    ? activity.sample_elapsed_seconds
    : []
  const distanceProgressSeries = Array.isArray(
    activity?.sample_distance_progress,
  )
    ? activity.sample_distance_progress
    : []

  const interpolatedProgress = getInterpolatedSeriesValue(
    elapsedSeries,
    distanceProgressSeries,
    elapsedSecond,
  )

  if (Number.isFinite(interpolatedProgress)) {
    return clamp(interpolatedProgress, 0, 1)
  }

  if (elapsedSeries.length <= 1) {
    return 0
  }

  const safeElapsed = clamp(
    Number(elapsedSecond) || 0,
    elapsedSeries[0] ?? 0,
    elapsedSeries[elapsedSeries.length - 1] ?? 0,
  )
  const totalElapsed =
    (elapsedSeries[elapsedSeries.length - 1] ?? 0) - (elapsedSeries[0] ?? 0)

  if (totalElapsed <= 0) {
    return 0
  }

  return clamp((safeElapsed - (elapsedSeries[0] ?? 0)) / totalElapsed, 0, 1)
}

/**
 * Returns series value at progress.
 *
 * @param {*} series - Value for series.
 * @param {*} progress01 - Normalized progress value between 0 and 1.
 * @returns {*} Requested value or structure.
 */
export function getSeriesValueAtProgress(series, progress01) {
  if (!Array.isArray(series) || !series.length) {
    return null
  }

  const clampedProgress = clamp(Number(progress01) || 0, 0, 1)
  const scaledIndex = clampedProgress * (series.length - 1)
  const startIndex = Math.floor(scaledIndex)
  const endIndex = Math.min(startIndex + 1, series.length - 1)
  const mix = scaledIndex - startIndex
  const startValue = Number(series[startIndex])
  const endValue = Number(series[endIndex])

  if (!Number.isFinite(startValue) && !Number.isFinite(endValue)) {
    return null
  }

  if (!Number.isFinite(startValue)) {
    return endValue
  }

  if (!Number.isFinite(endValue)) {
    return startValue
  }

  return startValue + (endValue - startValue) * mix
}
