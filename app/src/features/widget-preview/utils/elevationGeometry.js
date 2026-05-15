/**
 * Elevation geometry utilities — smoothing, normalization, projection
 * and SVG path generation for elevation overlay widgets.
 */

import {
  ELEVATION_FALLBACK_PADDING,
  VERTICAL_SCALE_CLAMP_MIN,
  VERTICAL_SCALE_CLAMP_MAX,
  DENSITY_CLAMP_MIN,
  DENSITY_CLAMP_MAX,
  SIMPLIFY_TOLERANCE_CLAMP_MAX,
  GEOMETRY_EPSILON,
} from '@/features/overlay-editor'
import { clamp } from '@/lib/geometryUtils'

export function normalizeElevationGeometry(
  values,
  width,
  height,
  margin = 0,
  verticalScale = 1,
  progressValues = [],
  targetDensity = 0.75,
  simplifyTolerancePx = 1,
) {
  const samples = values.reduce((result, value, index) => {
    if (!Number.isFinite(value)) {
      return result
    }

    const progressValue = Number(progressValues[index])
    result.push({
      progress: Number.isFinite(progressValue) ? clamp(progressValue, 0, 1) : values.length > 1 ? index / (values.length - 1) : 0,
      value: Number(value),
    })
    return result
  }, [])

  if (!samples.length) {
    const fallbackPadding = ELEVATION_FALLBACK_PADDING
    const fallbackPoints = [
      [fallbackPadding, height - fallbackPadding],
      [width * 0.32, height * 0.55],
      [width * 0.62, height * 0.36],
      [width - fallbackPadding, height * 0.48],
    ]
    return {
      points: fallbackPoints,
      progressValues: fallbackPoints.map((_, index) => (fallbackPoints.length > 1 ? index / (fallbackPoints.length - 1) : 0)),
    }
  }

  const safeMargin = Number.isFinite(Number(margin)) ? Number(margin) : 0
  const innerWidth = Math.max(width * (1 - 2 * safeMargin), 1)
  const innerHeight = Math.max(height * (1 - 2 * safeMargin), 1)
  const safeVerticalScale = clamp(Number(verticalScale) || 1, VERTICAL_SCALE_CLAMP_MIN, VERTICAL_SCALE_CLAMP_MAX)
  const safeTargetDensity = clamp(Number(targetDensity) || 0.75, DENSITY_CLAMP_MIN, DENSITY_CLAMP_MAX)
  const safeSimplifyTolerance = clamp(Number(simplifyTolerancePx) || 0, 0, SIMPLIFY_TOLERANCE_CLAMP_MAX)

  const smoothElevationSamples = (inputSamples) => {
    const coefficients = [-36, 9, 44, 69, 84, 89, 84, 69, 44, 9, -36]
    const radius = Math.floor(coefficients.length / 2)

    return inputSamples.map((sample, index) => {
      let total = 0
      let coefficientTotal = 0

      for (let offset = -radius; offset <= radius; offset += 1) {
        const neighborIndex = index + offset
        if (neighborIndex < 0 || neighborIndex >= inputSamples.length) {
          continue
        }

        const neighborValue = inputSamples[neighborIndex].value
        if (!Number.isFinite(neighborValue)) {
          continue
        }

        const coefficient = coefficients[offset + radius]
        total += neighborValue * coefficient
        coefficientTotal += coefficient
      }

      return {
        ...sample,
        value: Math.abs(coefficientTotal) <= GEOMETRY_EPSILON ? sample.value : total / coefficientTotal,
        preserve: index === 0 || index === inputSamples.length - 1,
      }
    })
  }

  const downsampleElevationSamples = (inputSamples, targetCount) => {
    if (inputSamples.length <= targetCount || targetCount < 3) {
      return inputSamples.map((sample, index) => ({
        ...sample,
        preserve: index === 0 || index === inputSamples.length - 1,
      }))
    }

    const smoothedSamples = smoothElevationSamples(inputSamples)
    const lastIndex = smoothedSamples.length - 1
    const selectedSamples = []

    for (let sampleIndex = 0; sampleIndex < targetCount; sampleIndex += 1) {
      const sourceIndex = Math.round((sampleIndex * lastIndex) / Math.max(targetCount - 1, 1))
      const nextSample = smoothedSamples[Math.min(sourceIndex, lastIndex)]
      if (selectedSamples.length > 0 && selectedSamples[selectedSamples.length - 1].progress === nextSample.progress) {
        continue
      }
      selectedSamples.push(nextSample)
    }

    if (selectedSamples.length === 1 && smoothedSamples.length > 1) {
      selectedSamples.push(smoothedSamples[smoothedSamples.length - 1])
    }

    return selectedSamples
  }

  const simplifyProjectedPointsSegment = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
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
    for (let index = 1; index < inputPoints.length - 1; index += 1) {
      const distance = perpendicularDistance(inputPoints[index], inputPoints[0], inputPoints[inputPoints.length - 1])
      if (distance > maxDistance) {
        maxDistance = distance
        splitIndex = index
      }
    }

    if (maxDistance <= tolerance) {
      return [inputPoints[0], inputPoints[inputPoints.length - 1]]
    }

    const left = simplifyProjectedPointsSegment(inputPoints.slice(0, splitIndex + 1), tolerance)
    const right = simplifyProjectedPointsSegment(inputPoints.slice(splitIndex), tolerance)
    return [...left.slice(0, -1), ...right]
  }

  const simplifyProjectedPoints = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
    }

    const preservedIndexes = inputPoints.reduce((result, point, index) => {
      if (point.preserve) result.push(index)
      return result
    }, [])
    if (preservedIndexes.length >= 2) {
      const result = []
      for (let windowIndex = 0; windowIndex < preservedIndexes.length - 1; windowIndex += 1) {
        const start = preservedIndexes[windowIndex]
        const end = preservedIndexes[windowIndex + 1]
        const simplifiedSegment = simplifyProjectedPointsSegment(inputPoints.slice(start, end + 1), tolerance)
        if (result.length === 0) {
          result.push(...simplifiedSegment)
        } else {
          result.push(...simplifiedSegment.slice(1))
        }
      }
      return result
    }

    return simplifyProjectedPointsSegment(inputPoints, tolerance)
  }

  const targetCount = Math.max(2, Math.min(samples.length, Math.round(width * safeTargetDensity)))
  const downsampledSamples = downsampleElevationSamples(samples, targetCount)
  const usableValues = downsampledSamples.map((sample) => sample.value)
  const minimum = Math.min(...usableValues)
  const maximum = Math.max(...usableValues)
  const amplitude = Math.max(maximum - minimum, 1e-9)

  const projectedPoints = downsampledSamples.map((sample) => {
    const progress = Number.isFinite(sample.progress) ? clamp(sample.progress, 0, 1) : 0
    const x = width * safeMargin + innerWidth * progress
    const normalized = amplitude <= 0 ? 0.5 : (sample.value - minimum) / amplitude
    const centered = clamp((normalized - 0.5) * safeVerticalScale + 0.5, 0, 1)
    const y = height - (height * safeMargin + innerHeight * centered)
    return {
      point: [x, y],
      progress,
      preserve: sample.preserve === true,
    }
  })

  const simplified = simplifyProjectedPoints(projectedPoints, safeSimplifyTolerance)

  return {
    points: simplified.map(({ point }) => point),
    progressValues: simplified.map(({ progress }) => progress),
  }
}
