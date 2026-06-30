/**
 * Placeholder plot geometry for previews without activity data.
 *
 * These shapes restore the old frontend-only fallback behavior that existed
 * before route/elevation geometry moved fully into Rust.
 */

import { areaToSvg, findPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { buildElevationCompletedPoints, buildRouteFramePreview } from './svgPreviewUtils'

function buildLinearProgressValues(points) {
  return points.map((_, index) => (points.length > 1 ? index / (points.length - 1) : 0))
}

function buildPlaceholderRoutePoints(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

function buildPlaceholderElevationPoints(width, height) {
  const padding = 8
  return [
    [padding, height - padding],
    [width * 0.32, height * 0.55],
    [width * 0.62, height * 0.36],
    [width - padding, height * 0.48],
  ]
}

function buildPlaceholderProgress(points) {
  return buildLinearProgressValues(points)
}

function resolvePlaceholderProgress(previewSecond, fallbackDurationSeconds) {
  const safeDuration = Number(fallbackDurationSeconds)
  if (!Number.isFinite(safeDuration) || safeDuration <= 0) {
    return 0
  }

  const safeSecond = Number(previewSecond)
  if (!Number.isFinite(safeSecond)) {
    return 0
  }

  return Math.min(Math.max(safeSecond / safeDuration, 0), 1)
}

export function buildPlaceholderRoutePreviewGeometry({ width, height, previewSecond, fallbackDurationSeconds }) {
  const points = buildPlaceholderRoutePoints(width, height)
  const progressValues = buildPlaceholderProgress(points)
  const progress01 = resolvePlaceholderProgress(previewSecond, fallbackDurationSeconds)
  const { markerPoint, completedPoints } = buildRouteFramePreview(points, progressValues, progress01)

  return {
    markerPoint,
    remainingSvgPoints: pointsToSvg(points),
    completedSvgPoints: pointsToSvg(completedPoints),
  }
}

export function buildPlaceholderElevationPreviewGeometry({ width, height, previewSecond, fallbackDurationSeconds }) {
  const points = buildPlaceholderElevationPoints(width, height)
  const progressValues = buildPlaceholderProgress(points)
  const progress01 = resolvePlaceholderProgress(previewSecond, fallbackDurationSeconds)
  const markerHit = findPointAtProgress(points, progressValues, progress01)
  const completedPoints = buildElevationCompletedPoints(points, progressValues, progressValues, progress01, progress01)

  return {
    markerPoint: markerHit?.point ?? points[points.length - 1] ?? null,
    elevationValue: null,
    remainingSvgPoints: pointsToSvg(points),
    completedSvgPoints: pointsToSvg(completedPoints),
    areaSvgPoints: areaToSvg(points, width, height, null),
    completedAreaSvgPoints: areaToSvg(completedPoints, width, height, null),
  }
}
