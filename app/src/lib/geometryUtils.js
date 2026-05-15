/**
 * Shared geometry utilities — point interpolation, SVG path generation,
 * and widget transform helpers used across route and elevation domains.
 *
 * Domain-agnostic pure functions extracted from features/overlay-editor.
 */

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {number} value - Input value.
 * @param {number} min - Lower bound.
 * @param {number} max - Upper bound.
 * @returns {number} Clamped value within [min, max].
 */
export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Builds a CSS transform string from scale and rotation values.
 * Only includes non-default values to minimize transform property size.
 *
 * @param {object} options
 * @param {number} [options.scale=1] - Uniform scale multiplier.
 * @param {number} [options.rotation=0] - Rotation in degrees.
 * @returns {string|undefined} CSS transform string, or undefined if no transform needed.
 */
export function buildWidgetTransform({ scale = 1, rotation = 0 }) {
  const transforms = []

  if (rotation) {
    transforms.push(`rotate(${rotation}deg)`)
  }

  if (scale !== 1) {
    transforms.push(`scale(${scale})`)
  }

  return transforms.length ? transforms.join(' ') : undefined
}

/**
 * Converts an array of [x, y] point pairs to an SVG points string.
 *
 * @param {number[][]} points - Array of [x, y] coordinates.
 * @returns {string} Space-separated "x,y" pairs.
 */
export function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

/**
 * Interpolates a point along a polyline at the given normalized progress.
 * Falls back to the last point if progress exceeds the array bounds.
 *
 * @param {number[][]} points - Array of [x, y] coordinates.
 * @param {number} progress01 - Normalized progress between 0 and 1.
 * @returns {number[]|null} Interpolated [x, y] point or null if empty.
 */
export function getPointAtProgress(points, progress01) {
  if (!points.length) {
    return null
  }

  if (points.length === 1) {
    return points[0]
  }

  const clampedProgress = clamp(Number(progress01) || 0, 0, 1)
  const scaledIndex = clampedProgress * (points.length - 1)
  const startIndex = Math.floor(scaledIndex)
  const endIndex = Math.min(startIndex + 1, points.length - 1)
  const mix = scaledIndex - startIndex
  const startPoint = points[startIndex]
  const endPoint = points[endIndex]

  if (!startPoint || !endPoint) {
    return points[Math.min(startIndex, points.length - 1)] || null
  }

  return [startPoint[0] + (endPoint[0] - startPoint[0]) * mix, startPoint[1] + (endPoint[1] - startPoint[1]) * mix]
}

/**
 * Interpolates a point along a polyline using per-point metric progress values
 * (e.g. distance-based progress rather than index-based).
 *
 * @param {number[][]} points - Array of [x, y] coordinates.
 * @param {number[]} progressValues - Per-point progress values (0–1).
 * @param {number} targetProgress - Target progress to interpolate at.
 * @returns {number[]|null} Interpolated [x, y] point or null.
 */
export function getPointAtMetricProgress(points, progressValues, targetProgress) {
  const result = getPointAtMetricProgressWithIndex(points, progressValues, targetProgress)

  return result ? result.point : null
}

/**
 * Interpolates a point and returns both the point and the right-side segment
 * index using per-point metric progress values. Used for determining which
 * segment of the polyline the "completed" portion ends at.
 *
 * @param {number[][]} points - Array of [x, y] coordinates.
 * @param {number[]} progressValues - Per-point progress values (0–1).
 * @param {number} targetProgress - Target progress to interpolate at.
 * @returns {{ index: number, point: number[] }|null} Point with segment index or null.
 */
export function getPointAtMetricProgressWithIndex(points, progressValues, targetProgress) {
  if (!Array.isArray(points) || !Array.isArray(progressValues) || !points.length) {
    return null
  }

  const safeTargetProgress = clamp(Number(targetProgress) || 0, 0, 1)
  let firstValidIndex = -1
  let lastValidIndex = -1

  for (let index = 0; index < points.length; index += 1) {
    if (points[index] && Number.isFinite(points[index][0]) && Number.isFinite(points[index][1]) && Number.isFinite(progressValues[index])) {
      firstValidIndex = index
      break
    }
  }

  if (firstValidIndex === -1) {
    const point = getPointAtProgress(points, safeTargetProgress)
    return point ? { index: 0, point } : null
  }

  for (let index = points.length - 1; index >= 0; index -= 1) {
    if (points[index] && Number.isFinite(points[index][0]) && Number.isFinite(points[index][1]) && Number.isFinite(progressValues[index])) {
      lastValidIndex = index
      break
    }
  }

  if (safeTargetProgress <= progressValues[firstValidIndex]) {
    return {
      index: Math.min(firstValidIndex + 1, points.length - 1),
      point: points[firstValidIndex],
    }
  }

  if (safeTargetProgress >= progressValues[lastValidIndex]) {
    return { index: lastValidIndex, point: points[lastValidIndex] }
  }

  let leftIndex = firstValidIndex
  let rightIndex = firstValidIndex

  for (let index = firstValidIndex + 1; index <= lastValidIndex; index += 1) {
    const nextProgress = Number(progressValues[index])
    if (!Number.isFinite(nextProgress)) {
      continue
    }

    if (nextProgress >= safeTargetProgress) {
      rightIndex = index
      break
    }

    leftIndex = index
  }

  const leftProgress = Number(progressValues[leftIndex])
  const rightProgress = Number(progressValues[rightIndex])
  const leftPoint = points[leftIndex]
  const rightPoint = points[rightIndex]

  if (!Number.isFinite(leftProgress) || !Number.isFinite(rightProgress) || !leftPoint || !rightPoint) {
    return null
  }

  if (rightIndex === leftIndex || rightProgress === leftProgress) {
    return { index: rightIndex, point: leftPoint }
  }

  const ratio = (safeTargetProgress - leftProgress) / (rightProgress - leftProgress)

  return {
    index: rightIndex,
    point: [leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio, leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio],
  }
}

/**
 * Converts a polyline to a closed SVG polygon points string by adding
 * baseline corners at the given padding from the bottom.
 *
 * @param {number[][]} points - Array of [x, y] coordinates.
 * @param {number} _width - Width (unused, kept for signature compatibility).
 * @param {number} height - Height for baseline positioning.
 * @param {number} [padding=18] - Baseline inset from bottom.
 * @returns {string} SVG points string for a filled polygon.
 */
export function areaToSvg(points, _width, height, padding = 18) {
  if (!points.length) return ''
  const baseline = Number.isFinite(padding) ? height - padding : height
  return [`${points[0][0]},${baseline}`, ...points.map(([x, y]) => `${x},${y}`), `${points[points.length - 1][0]},${baseline}`].join(' ')
}
