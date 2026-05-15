/**
 * Numeric interpolation utilities for activity data series.
 * Provides linear interpolation between sample points for elapsed-time series.
 *
 * Domain-agnostic pure functions extracted from features/overlay-editor.
 */

/**
 * Interpolates a numeric series at the target time.
 *
 * @param {number[]} elapsedSeries - Sample elapsed seconds.
 * @param {number[]} values - Numeric series aligned with elapsed samples.
 * @param {number} targetSecond - Requested elapsed second.
 * @returns {number|null} Interpolated numeric value.
 */
export function interpolateNumericSeries(elapsedSeries, values, targetSecond) {
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
 * Interpolates a course point (lat/lng pair) at the target time.
 *
 * @param {number[]} elapsedSeries - Sample elapsed seconds.
 * @param {number[][]} coursePoints - Course point series aligned with elapsed samples.
 * @param {number} targetSecond - Requested elapsed second.
 * @returns {number[]|null} Interpolated [latitude, longitude] or null.
 */
export function interpolateCoursePoint(elapsedSeries, coursePoints, targetSecond) {
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
 * @param {number[]} left - Left-hand point.
 * @param {number[]} right - Right-hand point.
 * @returns {boolean} Whether the points match.
 */
export function coursePointsEqual(left, right) {
  return Array.isArray(left) && Array.isArray(right) && left[0] === right[0] && left[1] === right[1]
}
