/**
 * Route geometry utilities — point fitting, simplification, downsampling,
 * and normalization for route/course overlay widgets.
 */

import { DENSITY_CLAMP_MIN, DENSITY_CLAMP_MAX, SIMPLIFY_MIN_TOLERANCE, ROUTE_FALLBACK_INSET_MAX_RATIO } from '@/features/overlay-editor'
import { clamp } from '@/lib/geometryUtils'

function buildFallbackRoute(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

function fitPointsToWidget(points, width, height, insetPx, invertY = true) {
  if (!points.length) {
    return []
  }

  const minX = Math.min(...points.map(([x]) => x))
  const maxX = Math.max(...points.map(([x]) => x))
  const minY = Math.min(...points.map(([, y]) => y))
  const maxY = Math.max(...points.map(([, y]) => y))
  const safeInset = Math.min(Math.max(Number(insetPx) || 0, 0), Math.min(width, height) * ROUTE_FALLBACK_INSET_MAX_RATIO)
  const innerWidth = Math.max(width - safeInset * 2, 1)
  const innerHeight = Math.max(height - safeInset * 2, 1)
  const spanX = Math.max(maxX - minX, 0.000001)
  const spanY = Math.max(maxY - minY, 0.000001)
  const scale = Math.min(innerWidth / spanX, innerHeight / spanY)
  const offsetX = (width - spanX * scale) / 2
  const offsetY = (height - spanY * scale) / 2

  return points.map(([x, y]) => {
    const fittedX = (x - minX) * scale + offsetX
    let fittedY = (y - minY) * scale + offsetY
    if (invertY) {
      fittedY = height - fittedY
    }
    return [fittedX, fittedY]
  })
}

function routeGeometryInsetPx(widgetWidth, widgetHeight, lineWidth, completedLineWidth, markerSize) {
  const safeWidth = Number(lineWidth) || 0
  const safeCompletedWidth = Number(completedLineWidth) || 0
  const safeMarkerSize = Number(markerSize) || 0
  const lineInset = Math.max(safeWidth, safeCompletedWidth) * 0.5
  return Math.min(Math.max(safeMarkerSize, lineInset) + 1, Math.min(widgetWidth, widgetHeight) * ROUTE_FALLBACK_INSET_MAX_RATIO)
}

function simplifyRouteSamples(samples, tolerance) {
  if (samples.length <= 2 || tolerance <= 0) {
    return samples
  }

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

function downsampleRouteSamples(samples, targetCount) {
  if (samples.length <= targetCount || targetCount < 3) {
    return samples
  }

  const bucketSize = (samples.length - 2) / (targetCount - 2)
  const sampled = [samples[0]]
  let a = 0

  for (let bucketIndex = 0; bucketIndex < targetCount - 2; bucketIndex += 1) {
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

    a = nextA
    sampled.push(samples[a])
  }

  sampled.push(samples[samples.length - 1])
  return sampled
}

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
  const safeTargetDensity = clamp(Number(targetDensity) || 1, DENSITY_CLAMP_MIN, DENSITY_CLAMP_MAX)
  const targetCount = Math.max(2, Math.min(fittedSamples.length, Math.round(width * safeTargetDensity)))
  const downsampled = downsampleRouteSamples(fittedSamples, targetCount)
  const simplified = simplifyRouteSamples(downsampled, Math.max(Number(simplifyTolerancePx) || 1, SIMPLIFY_MIN_TOLERANCE))

  return {
    points: simplified.map((sample) => sample.point),
    progressValues: simplified.map((sample) => sample.progress),
  }
}

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

export function getCompletedIndex(totalPoints, sampleIndex, progress01) {
  if (totalPoints <= 1) return 0

  if (Number.isFinite(progress01)) {
    return clamp(Math.floor(progress01 * (totalPoints - 1)), 0, totalPoints - 1)
  }

  return clamp(sampleIndex, 0, totalPoints - 1)
}
