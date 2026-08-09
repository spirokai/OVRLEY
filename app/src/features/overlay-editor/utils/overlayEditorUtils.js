/**
 * Overlay editor utilities — scene size, activity data interpolation,
 * grid size computation, and series-value extraction.
 *
 * Pure functions. No React imports, no side effects.
 *
 * @module overlayEditorUtils
 */

import { clamp } from '@/lib/utils'
import { DEFAULT_ACTIVITY_PREVIEW } from '../data/overlayEditorConfig'
import { EDITOR_GRID_DIVISIONS } from '../data/overlayEditorConstants'
import { interpolateNumericSeries, MISSING_SAMPLE_POLICY } from '@/lib/interpolation'
import { getStandardMetricDefinition, getStandardMetricInterpolation } from '@/lib/widget/standard-metrics'

/**
 * Returns the configured scene dimensions with defaults of 1920x1080.
 *
 * @param {object|null} config - Overlay template config.
 * @returns {{ width: number, height: number }} Scene dimensions.
 */
export function getSceneSize(config) {
  return {
    width: config?.scene?.width || 1920,
    height: config?.scene?.height || 1080,
  }
}

/**
 * Returns the held value at the target X.
 * Finds the sample with the largest X <= targetX and returns its Y value.
 * If the target is before the first valid sample, clamps to that first sample
 * so preview behavior matches render boundary interpolation.
 *
 * @param {number[]} xValues - X-axis sample values (monotonic).
 * @param {number[]} yValues - Y-axis sample values aligned with xValues.
 * @param {number} targetX - Requested X value.
 * @returns {number|string|null} Held value, or null if no valid sample exists before targetX.
 */
export function getHoldSeriesValue(xValues, yValues, targetX) {
  if (!Array.isArray(xValues) || !Array.isArray(yValues) || !xValues.length) {
    return null
  }

  const safeTargetX = Number(targetX)
  if (!Number.isFinite(safeTargetX)) {
    return null
  }

  // Find the rightmost index where xValues[index] <= safeTargetX
  let bestIndex = -1
  let low = 0
  let high = xValues.length - 1

  while (low <= high) {
    const mid = Math.floor((low + high) / 2)
    if (Number(xValues[mid]) <= safeTargetX) {
      bestIndex = mid
      low = mid + 1
    } else {
      high = mid - 1
    }
  }

  if (bestIndex === -1) {
    for (let i = 0; i < xValues.length && i < yValues.length; i += 1) {
      if (Number.isFinite(xValues[i]) && yValues[i] !== null && yValues[i] !== undefined) {
        return yValues[i]
      }
    }

    return null
  }

  // Walk backward from bestIndex to find the first non-null Y value
  for (let i = bestIndex; i >= 0; i -= 1) {
    if (yValues[i] !== null && yValues[i] !== undefined) {
      return yValues[i]
    }
  }

  return null
}

/**
 * Returns the canonical activity series for a metric type.
 * Standard metrics use their manifest dataSource; altitude uses the preferred
 * barometric series when present and otherwise falls back to elevation.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {string} metricType - Metric or raw activity series key.
 * @returns {Array<unknown>|undefined} The selected series, or undefined when absent.
 */
export function getMetricSeries(activity, metricType) {
  const activityKey = getStandardMetricDefinition(metricType)?.dataSource ?? metricType
  return metricType === 'altitude' ? getPreferredElevationSeries(activity) : activity?.[activityKey]
}

/**
 * Supplies activity data only while the current preview second belongs to it.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {number} previewSecond - Current preview second.
 * @returns {object|null} Activity data available at the preview second.
 */
export function getPreviewActivity(activity, previewSecond) {
  if (activity === null) return null

  const activityEnd = activity.trim_end_seconds
  return previewSecond >= 0 && previewSecond <= activityEnd ? activity : null
}

/**
 * Interpolates an activity metric series (speed, heartrate, etc.) at the
 * given elapsed second. Falls back to DEFAULT_ACTIVITY_PREVIEW values.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {string} key - Activity series key (e.g. 'speed', 'heartrate').
 * @param {number} elapsedSecond - Target elapsed second.
 * @returns {number|string|null} Interpolated value or preview default.
 */
export function getInterpolatedActivityValue(activity, key, elapsedSecond) {
  const previewActivity = getPreviewActivity(activity, elapsedSecond)
  const elapsedSeries = Array.isArray(previewActivity?.sample_elapsed_seconds) ? previewActivity.sample_elapsed_seconds : []
  const series = getMetricSeries(previewActivity, key)

  if (!Array.isArray(series) || !series.length || !elapsedSeries.length) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  const interpolationMode = getStandardMetricInterpolation(key)

  if (interpolationMode === 'hold') {
    const heldValue = getHoldSeriesValue(elapsedSeries, series, elapsedSecond)
    return heldValue ?? null
  }

  const policy = interpolationMode === 'preserve' ? MISSING_SAMPLE_POLICY.PRESERVE : MISSING_SAMPLE_POLICY.BRIDGE
  const interpolatedValue = interpolateNumericSeries(elapsedSeries, series, elapsedSecond, policy)

  if (interpolatedValue !== null) return interpolatedValue
  if (interpolationMode === 'preserve') return 0
  return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

function getPreferredElevationSeries(activity) {
  const barometricSeries = activity?.barometric_altitude
  if (Array.isArray(barometricSeries) && barometricSeries.some((value) => value !== null && value !== undefined)) {
    return barometricSeries
  }

  return activity?.elevation
}

/**
 * Interpolates the time-of-day value at the given elapsed second.
 * Interpolates the ISO time series when available, falling back to sync_time
 * only when the activity has no time series values.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {number} elapsedSecond - Target elapsed second.
 * @returns {string} ISO timestamp string.
 */
export function getInterpolatedTimeValue(activity, elapsedSecond) {
  const previewActivity = getPreviewActivity(activity, elapsedSecond)
  if (previewActivity === null) return DEFAULT_ACTIVITY_PREVIEW.time

  const elapsedSeries = Array.isArray(previewActivity.sample_elapsed_seconds) ? previewActivity.sample_elapsed_seconds : []
  const timeSeries = Array.isArray(previewActivity.time) ? previewActivity.time : []
  const numericTimeSeries = timeSeries.map((value) => {
    const parsed = Date.parse(value || '')
    return Number.isFinite(parsed) ? parsed : null
  })
  const interpolatedTimeMs = interpolateNumericSeries(elapsedSeries, numericTimeSeries, elapsedSecond)

  if (Number.isFinite(interpolatedTimeMs)) {
    return new Date(interpolatedTimeMs).toISOString()
  }

  if (numericTimeSeries.every((value) => value === null)) {
    const syncTimeMs = Date.parse(previewActivity.sync_time || '')
    if (Number.isFinite(syncTimeMs)) {
      return new Date(syncTimeMs + Math.max(elapsedSecond, 0) * 1000).toISOString()
    }
  }

  return DEFAULT_ACTIVITY_PREVIEW.time
}

/**
 * Returns the distance-based progress (0–1) at the given elapsed second.
 * Uses the sample_distance_progress series if available, otherwise
 * falls back to a linear ratio of elapsed time.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {number} elapsedSecond - Target elapsed second.
 * @returns {number} Normalized progress between 0 and 1.
 */
export function getDistanceProgressAtElapsed(activity, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgressSeries = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  const interpolatedProgress = interpolateNumericSeries(elapsedSeries, distanceProgressSeries, elapsedSecond)

  if (Number.isFinite(interpolatedProgress)) {
    return clamp(interpolatedProgress, 0, 1)
  }

  if (elapsedSeries.length <= 1) {
    return 0
  }

  const safeElapsed = clamp(Number(elapsedSecond) || 0, elapsedSeries[0] ?? 0, elapsedSeries[elapsedSeries.length - 1] ?? 0)
  const totalElapsed = (elapsedSeries[elapsedSeries.length - 1] ?? 0) - (elapsedSeries[0] ?? 0)

  if (totalElapsed <= 0) {
    return 0
  }

  return clamp((safeElapsed - (elapsedSeries[0] ?? 0)) / totalElapsed, 0, 1)
}

/**
 * Returns an interpolated series value at normalized progress (0–1).
 * Linearly interpolates between adjacent samples at the progress position.
 *
 * @param {number[]} series - Numeric series to interpolate.
 * @param {number} progress01 - Normalized progress between 0 and 1.
 * @returns {number|null} Interpolated value or null if series is empty.
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

/**
 * Computes the editor overlay grid cell size by dividing the smaller
 * scene dimension by EDITOR_GRID_DIVISIONS.
 *
 * @param {{ width: number, height: number }} sceneSize - Scene dimensions.
 * @returns {number} Grid cell size in scene-space pixels.
 */
export function getEditorGridSize(sceneSize) {
  const width = Number(sceneSize?.width)
  const height = Number(sceneSize?.height)

  if (!Number.isFinite(width) || !Number.isFinite(height)) {
    return 1
  }

  return Math.max(1, Math.round(Math.min(width, height) / EDITOR_GRID_DIVISIONS))
}
