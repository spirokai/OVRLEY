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
import { getStandardMetricInterpolation } from '@/lib/widget/standard-metrics'

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

function isValidInterpolatedSample(xValues, yValues, index) {
  return Number.isFinite(xValues[index]) && Number.isFinite(yValues[index])
}

function findNearestValidSampleIndex(xValues, yValues, startIndex, direction) {
  for (let index = startIndex; index >= 0 && index < xValues.length; index += direction) {
    if (isValidInterpolatedSample(xValues, yValues, index)) {
      return index
    }
  }

  return -1
}

function findFirstIndexAtOrAfter(xValues, targetX, low, high) {
  let left = low
  let right = high
  let result = high

  while (left <= right) {
    const middle = Math.floor((left + right) / 2)
    const middleX = Number(xValues[middle])

    if (Number.isFinite(middleX) && middleX >= targetX) {
      result = middle
      right = middle - 1
    } else {
      left = middle + 1
    }
  }

  return result
}

/**
 * Performs linear interpolation on a series of (x, y) values at the target X.
 * Falls back to the nearest endpoint if targetX is out of range.
 *
 * @param {number[]} xValues - X-axis sample values (monotonic).
 * @param {number[]} yValues - Y-axis sample values aligned with xValues.
 * @param {number} targetX - Requested X value to interpolate at.
 * @returns {number|null} Interpolated Y value or null if no valid samples.
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

  const insertionIndex = findFirstIndexAtOrAfter(xValues, safeTargetX, firstValidIndex, lastValidIndex)
  const rightIndex = findNearestValidSampleIndex(xValues, yValues, insertionIndex, 1)
  const rightXAtInsertion = Number(xValues[rightIndex])
  const leftIndex = findNearestValidSampleIndex(
    xValues,
    yValues,
    rightXAtInsertion === safeTargetX ? rightIndex : Math.min(rightIndex - 1, lastValidIndex),
    -1,
  )

  const leftX = Number(xValues[leftIndex])
  const rightX = Number(xValues[rightIndex])
  const leftY = Number(yValues[leftIndex])
  const rightY = Number(yValues[rightIndex])

  if (!Number.isFinite(leftX) || !Number.isFinite(rightX) || !Number.isFinite(leftY) || !Number.isFinite(rightY)) {
    return null
  }

  if (rightIndex === leftIndex || rightX === leftX) {
    return leftY
  }

  const ratio = (safeTargetX - leftX) / (rightX - leftX)
  return leftY + (rightY - leftY) * ratio
}

/**
 * Returns the last known value at or before the target X using hold semantics.
 * Finds the sample with the largest X <= targetX and returns its Y value.
 * Null Y values are skipped by walking backward from the insertion point.
 *
 * @param {number[]} xValues - X-axis sample values (monotonic).
 * @param {number[]} yValues - Y-axis sample values aligned with xValues.
 * @param {number} targetX - Requested X value.
 * @returns {number|null} Held Y value, or null if no valid sample exists before targetX.
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
    return null
  }

  // Walk backward from bestIndex to find the first non-null Y value
  for (let i = bestIndex; i >= 0; i -= 1) {
    if (Number.isFinite(yValues[i])) {
      return Number(yValues[i])
    }
  }

  return null
}

/**
 * Interpolates an activity metric series (speed, heartrate, etc.) at the
 * given elapsed second. Falls back to DEFAULT_ACTIVITY_PREVIEW values.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {string} key - Activity series key (e.g. 'speed', 'heartrate').
 * @param {number} elapsedSecond - Target elapsed second.
 * @returns {number|null} Interpolated value or preview default.
 */
export function getInterpolatedActivityValue(activity, key, elapsedSecond) {
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const series = activity?.[key]

  if (!Array.isArray(series) || !elapsedSeries.length) {
    return DEFAULT_ACTIVITY_PREVIEW[key] ?? null
  }

  const interpolationMode = getStandardMetricInterpolation(key)

  if (interpolationMode === 'hold') {
    const heldValue = getHoldSeriesValue(elapsedSeries, series, elapsedSecond)
    return heldValue ?? null
  }

  const interpolatedValue = getInterpolatedSeriesValue(elapsedSeries, series, elapsedSecond)

  return interpolatedValue ?? DEFAULT_ACTIVITY_PREVIEW[key] ?? null
}

/**
 * Interpolates the time-of-day value at the given elapsed second.
 * Uses the source_start_time offset when available, otherwise
 * interpolates the ISO time series.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {number} elapsedSecond - Target elapsed second.
 * @returns {string} ISO timestamp string.
 */
export function getInterpolatedTimeValue(activity, elapsedSecond) {
  const sourceStartTimeMs = Date.parse(activity?.source_start_time || '')
  if (Number.isFinite(sourceStartTimeMs)) {
    return new Date(sourceStartTimeMs + Math.max(elapsedSecond, 0) * 1000).toISOString()
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const timeSeries = Array.isArray(activity?.time) ? activity.time : []
  const numericTimeSeries = timeSeries.map((value) => {
    const parsed = Date.parse(value || '')
    return Number.isFinite(parsed) ? parsed : null
  })
  const interpolatedTimeMs = getInterpolatedSeriesValue(elapsedSeries, numericTimeSeries, elapsedSecond)

  return Number.isFinite(interpolatedTimeMs) ? new Date(interpolatedTimeMs).toISOString() : DEFAULT_ACTIVITY_PREVIEW.time
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

  const interpolatedProgress = getInterpolatedSeriesValue(elapsedSeries, distanceProgressSeries, elapsedSecond)

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
