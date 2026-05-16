/**
 * Route geometry utilities — point fitting, simplification, downsampling,
 * and normalization for route/course overlay widgets.
 */

import { DENSITY_CLAMP_MIN, DENSITY_CLAMP_MAX, SIMPLIFY_MIN_TOLERANCE, ROUTE_FALLBACK_INSET_MAX_RATIO } from '@/features/overlay-editor'
import { clamp } from '@/lib/geometryUtils'

/**
 * Builds a synthetic fallback route when no valid GPS samples are available.
 * Returns 5 points forming a gentle S-curve within the widget bounds.
 *
 * @param {number} width - Widget width in pixels.
 * @param {number} height - Widget height in pixels.
 * @returns {number[][]} Array of 5 [x, y] points for the fallback route.
 */
function buildFallbackRoute(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

/**
 * Fits projected coordinate points into widget SVG bounds with uniform scale and optional Y-axis inversion.
 *
 * Computes the bounding box of the input points, calculates a uniform scale
 * that fits within the widget's inner area (accounting for inset padding),
 * and centers the result. Optionally inverts the Y axis to convert from
 * Cartesian to SVG screen coordinates.
 *
 * @param {number[][]} points - Array of [x, y] coordinate pairs in projected space.
 * @param {number} width - Widget width in pixels.
 * @param {number} height - Widget height in pixels.
 * @param {number} insetPx - Inset padding in pixels from widget edges.
 * @param {boolean} [invertY=true] - Whether to flip the Y axis for SVG coordinate system.
 * @returns {number[][]} Fitted [x, y] points in widget pixel coordinates.
 */
function fitPointsToWidget(points, width, height, insetPx, invertY = true) {
  if (!points.length) {
    return []
  }

  // Compute bounding box of input points in the projected coordinate space
  const minX = Math.min(...points.map(([x]) => x))
  const maxX = Math.max(...points.map(([x]) => x))
  const minY = Math.min(...points.map(([, y]) => y))
  const maxY = Math.max(...points.map(([, y]) => y))

  // Compute uniform scale and centering offset to fit points within the widget with safe inset
  const safeInset = Math.min(Math.max(Number(insetPx) || 0, 0), Math.min(width, height) * ROUTE_FALLBACK_INSET_MAX_RATIO)
  const innerWidth = Math.max(width - safeInset * 2, 1)
  const innerHeight = Math.max(height - safeInset * 2, 1)
  const spanX = Math.max(maxX - minX, 0.000001)
  const spanY = Math.max(maxY - minY, 0.000001)
  const scale = Math.min(innerWidth / spanX, innerHeight / spanY)
  const offsetX = (width - spanX * scale) / 2
  const offsetY = (height - spanY * scale) / 2

  // Transform each point using the computed scale and offset, optionally inverting the Y axis for SVG coordinate system
  return points.map(([x, y]) => {
    const fittedX = (x - minX) * scale + offsetX
    let fittedY = (y - minY) * scale + offsetY
    if (invertY) {
      fittedY = height - fittedY
    }
    return [fittedX, fittedY]
  })
}

/**
 * Computes a safe inset padding for route geometry based on line widths and marker size.
 *
 * Ensures that the thickest line and largest marker fit within the widget
 * without being clipped at the edges, capped at a fraction of the smaller dimension.
 *
 * @param {number} widgetWidth - Widget width in pixels.
 * @param {number} widgetHeight - Widget height in pixels.
 * @param {number} lineWidth - Remaining route line width in pixels.
 * @param {number} completedLineWidth - Completed route line width in pixels.
 * @param {number} markerSize - Marker diameter in pixels.
 * @returns {number} Computed inset in pixels.
 */
function routeGeometryInsetPx(widgetWidth, widgetHeight, lineWidth, completedLineWidth, markerSize) {
  const safeWidth = Number(lineWidth) || 0
  const safeCompletedWidth = Number(completedLineWidth) || 0
  const safeMarkerSize = Number(markerSize) || 0
  const lineInset = Math.max(safeWidth, safeCompletedWidth) * 0.5
  return Math.min(Math.max(safeMarkerSize, lineInset) + 1, Math.min(widgetWidth, widgetHeight) * ROUTE_FALLBACK_INSET_MAX_RATIO)
}

/**
 * Applies Ramer-Douglas-Peucker simplification to a polyline.
 *
 * Recursively removes points whose perpendicular distance from the line
 * segment between their neighbors is below the tolerance threshold.
 * Preserves the shape while reducing point count.
 *
 * @param {Array<{point: number[], progress: number}>} samples - Polyline samples with point coordinates.
 * @param {number} tolerance - Maximum allowed deviation in pixels.
 * @returns {Array<{point: number[], progress: number}>} Simplified samples.
 */
function simplifyRouteSamples(samples, tolerance) {
  // Guard — no simplification needed for 2 or fewer points or zero tolerance
  if (samples.length <= 2 || tolerance <= 0) {
    return samples
  }

  // Perpendicular distance — computes the shortest distance from a point to the line segment between start and end
  const perpendicularDistance = (point, start, end) => {
    const [x0, y0] = point.point
    const [x1, y1] = start.point
    const [x2, y2] = end.point
    const dx = x2 - x1
    const dy = y2 - y1
    if (Math.abs(dx) <= Number.EPSILON && Math.abs(dy) <= Number.EPSILON) {
      return Math.hypot(x0 - x1, y0 - y1)
    }
    return Math.abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / Math.hypot(dx, dy)
  }

  // Find the point furthest from the line segment between first and last — if within tolerance, collapse to endpoints
  let maxDistance = 0
  let splitIndex = 0
  for (let index = 1; index < samples.length - 1; index += 1) {
    const distance = perpendicularDistance(samples[index], samples[0], samples[samples.length - 1])
    if (distance > maxDistance) {
      maxDistance = distance
      splitIndex = index
    }
  }

  if (maxDistance <= tolerance) {
    return [samples[0], samples[samples.length - 1]]
  }

  const left = simplifyRouteSamples(samples.slice(0, splitIndex + 1), tolerance)
  const right = simplifyRouteSamples(samples.slice(splitIndex), tolerance)
  return [...left.slice(0, -1), ...right]
}

/**
 * Downsamples a polyline by selecting representative points using a triangle-area-maximizing algorithm.
 *
 * Divides the sample range into evenly-spaced buckets. For each bucket, computes
 * the average position of its points, then selects the point that forms the largest
 * triangle area with the average and the previously selected anchor point.
 * This preserves sharp corners and distinctive features better than uniform sampling.
 *
 * @param {Array<{point: number[], progress: number}>} samples - Polyline samples with point coordinates.
 * @param {number} targetCount - Desired number of points after downsampling.
 * @returns {Array<{point: number[], progress: number}>} Downsampled samples.
 */
function downsampleRouteSamples(samples, targetCount) {
  if (samples.length <= targetCount || targetCount < 3) {
    return samples
  }

  // Divide the sample range into evenly-spaced buckets, keeping the first point as the initial anchor
  const bucketSize = (samples.length - 2) / (targetCount - 2)
  const sampled = [samples[0]]
  let a = 0

  for (let bucketIndex = 0; bucketIndex < targetCount - 2; bucketIndex += 1) {
    // Compute the average position of points in the current bucket range — used as the reference for area maximisation
    const avgStart = Math.floor((bucketIndex + 1) * bucketSize) + 1
    const avgEnd = Math.min(samples.length, Math.floor((bucketIndex + 2) * bucketSize) + 1)
    const avgRangeStart = Math.min(avgStart, Math.max(avgEnd - 1, 0))
    const avgRange = samples.slice(avgRangeStart, avgEnd)
    const average =
      avgRange.length > 0
        ? {
            x: avgRange.reduce((sum, sample) => sum + sample.point[0], 0) / avgRange.length,
            y: avgRange.reduce((sum, sample) => sum + sample.point[1], 0) / avgRange.length,
          }
        : {
            x: samples[samples.length - 1].point[0],
            y: samples[samples.length - 1].point[1],
          }

    const rangeStart = Math.floor(bucketIndex * bucketSize) + 1
    const rangeEnd = Math.min(samples.length - 1, Math.floor((bucketIndex + 1) * bucketSize) + 1)
    const candidateStart = Math.min(rangeStart, samples.length - 2)
    const candidateEnd = Math.max(candidateStart + 1, rangeEnd)

    // Find the point within the candidate range that maximises the triangle area with the average and the previous anchor point
    let nextA = candidateStart
    let maxArea = -1
    for (let candidateIndex = candidateStart; candidateIndex < candidateEnd; candidateIndex += 1) {
      const pointA = samples[a].point
      const pointB = samples[candidateIndex].point
      const area = Math.abs((pointA[0] - average.x) * (pointB[1] - pointA[1]) - (pointA[0] - pointB[0]) * (average.y - pointA[1])) * 0.5
      if (area > maxArea) {
        maxArea = area
        nextA = candidateIndex
      }
    }

    // Push the selected point and advance the anchor for the next iteration
    a = nextA
    sampled.push(samples[a])
  }

  // Append the final point to complete the downsampled series
  sampled.push(samples[samples.length - 1])
  return sampled
}

/**
 * Normalizes route samples into projected SVG points with Mercator projection,
 * downsampling, and Ramer-Douglas-Peucker simplification.
 *
 * @param {Array<{point: number[], progress: number}>} samples - Route samples with [lat, lng] coordinates and progress.
 * @param {number} width - Target SVG width in pixels.
 * @param {number} height - Target SVG height in pixels.
 * @param {number} [targetDensity=1] - Target points-per-pixel density for downsampling.
 * @param {number} [simplifyTolerancePx=1] - Ramer-Douglas-Peucker simplification tolerance.
 * @param {number} [lineWidth=6] - Route line width in pixels (affects inset).
 * @param {number} [completedLineWidth=6] - Completed route line width (affects inset).
 * @param {number} [markerSize=18] - Marker size in pixels (affects inset).
 * @returns {{ points: number[][], progressValues: number[] }} Projected points and progress values.
 */
export function normalizeRouteGeometry(
  samples,
  width,
  height,
  targetDensity = 1,
  simplifyTolerancePx = 1,
  lineWidth = 6,
  completedLineWidth = 6,
  markerSize = 18,
) {
  // Validate samples — filter out entries with non-finite coordinates, falling back to a synthetic route if none remain
  const validSamples = samples.filter(
    (sample) => Array.isArray(sample?.point) && Number.isFinite(sample.point[0]) && Number.isFinite(sample.point[1]),
  )

  if (validSamples.length < 2) {
    const fallbackPoints = buildFallbackRoute(width, height)
    return {
      points: fallbackPoints,
      progressValues: fallbackPoints.map((_, index) => (fallbackPoints.length > 1 ? index / (fallbackPoints.length - 1) : 0)),
    }
  }

  // Mercator projection — compute mean latitude for equirectangular approximation, then fit projected points to widget bounds
  const validPoints = validSamples.map((sample) => sample.point)
  const latitudes = validPoints.map(([latitude]) => latitude)
  const meanLatitude = latitudes.reduce((sum, latitude) => sum + latitude, 0) / latitudes.length
  const meanLatitudeRadians = meanLatitude * (Math.PI / 180)
  const projectedPoints = validPoints.map(([latitude, longitude]) => [longitude * Math.cos(meanLatitudeRadians), latitude])
  const fitted = fitPointsToWidget(
    projectedPoints,
    width,
    height,
    routeGeometryInsetPx(width, height, lineWidth, completedLineWidth, markerSize),
    true,
  )
  const fittedSamples = validSamples.map((sample, index) => ({
    point: fitted[index],
    progress: Number.isFinite(sample.progress) ? clamp(sample.progress, 0, 1) : 0,
  }))
  // Downsample and simplify — reduce point count to target density, then apply Ramer-Douglas-Peucker simplification
  const safeTargetDensity = clamp(Number(targetDensity) || 1, DENSITY_CLAMP_MIN, DENSITY_CLAMP_MAX)
  const targetCount = Math.max(2, Math.min(fittedSamples.length, Math.round(width * safeTargetDensity)))
  const downsampled = downsampleRouteSamples(fittedSamples, targetCount)
  const simplified = simplifyRouteSamples(downsampled, Math.max(Number(simplifyTolerancePx) || 1, SIMPLIFY_MIN_TOLERANCE))

  return {
    points: simplified.map((sample) => sample.point),
    progressValues: simplified.map((sample) => sample.progress),
  }
}

/**
 * Normalizes raw point arrays (without progress) into fitted widget coordinates.
 *
 * Convenience wrapper that calls normalizeRouteGeometry with default parameters.
 *
 * @param {number[][]} points - Array of [latitude, longitude] coordinate pairs.
 * @param {number} width - Target SVG width in pixels.
 * @param {number} height - Target SVG height in pixels.
 * @param {number} [_padding=18] - Padding in pixels (passed as line width/marker size).
 * @returns {number[][]} Fitted points in widget coordinates.
 */
export function normalizeRoutePoints(points, width, height, _padding = 18) {
  return normalizeRouteGeometry(
    points.map((point, index) => ({
      point,
      progress: points.length > 1 ? index / (points.length - 1) : 0,
    })),
    width,
    height,
    1,
    1,
    6,
    6,
    18,
  ).points
}

/**
 * Returns the index in a progress-mapped array corresponding to a given progress value.
 *
 * @param {number} totalPoints - Total number of points in the array.
 * @param {number} sampleIndex - Fallback index if progress01 is not a finite number.
 * @param {number|null|undefined} progress01 - Progress value (0–1).
 * @returns {number} Clamped index (0 to totalPoints - 1).
 */
export function getCompletedIndex(totalPoints, sampleIndex, progress01) {
  if (totalPoints <= 1) return 0

  if (Number.isFinite(progress01)) {
    return clamp(Math.floor(progress01 * (totalPoints - 1)), 0, totalPoints - 1)
  }

  return clamp(sampleIndex, 0, totalPoints - 1)
}
