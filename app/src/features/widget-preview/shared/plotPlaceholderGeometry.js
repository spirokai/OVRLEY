/**
 * Placeholder plot geometry for previews without activity data.
 *
 * These shapes restore the old frontend-only fallback behavior that existed
 * before route/elevation geometry moved fully into Rust.
 */

import { areaToSvg, findPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { buildElevationCompletedPoints } from './svgPreviewUtils'

function buildLinearProgressValues(points) {
  return points.map((_, index) => (points.length > 1 ? index / (points.length - 1) : 0))
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

function resolvePlaceholderProgress(previewSecond, fallbackDurationSeconds) {
  return Math.min(Math.max(previewSecond / fallbackDurationSeconds, 0), 1)
}

/**
 * Builds static elevation placeholder geometry for the requested dimensions.
 *
 * @param {object} params
 * @param {number} params.width - Placeholder width.
 * @param {number} params.height - Placeholder height.
 * @returns {{ points: number[][], progressValues: number[], remainingSvgPoints: string, areaSvgPoints: string }} Static elevation geometry.
 */
export function buildPlaceholderElevationStaticGeometry({ width, height }) {
  const points = buildPlaceholderElevationPoints(width, height)
  const progressValues = buildLinearProgressValues(points)
  return {
    points,
    progressValues,
    remainingSvgPoints: pointsToSvg(points),
    areaSvgPoints: areaToSvg(points, width, height, null),
  }
}

/**
 * Builds the complete elevation placeholder geometry.
 *
 * @param {object} params
 * @param {number} params.width - Placeholder width.
 * @param {number} params.height - Placeholder height.
 * @param {number} params.previewSecond - Current preview timestamp.
 * @param {number} params.fallbackDurationSeconds - Placeholder timeline duration.
 * @param {object} [params.staticGeometry] - Memoized static geometry.
 * @returns {object} Complete elevation placeholder geometry.
 */
export function buildPlaceholderElevationPreviewGeometry({ width, height, previewSecond, fallbackDurationSeconds, staticGeometry = null }) {
  const geometry = staticGeometry ?? buildPlaceholderElevationStaticGeometry({ width, height })
  const progress01 = resolvePlaceholderProgress(previewSecond, fallbackDurationSeconds)
  const markerHit = findPointAtProgress(geometry.points, geometry.progressValues, progress01)
  const completedPoints = buildElevationCompletedPoints(geometry.points, geometry.progressValues, geometry.progressValues, progress01, progress01)

  return {
    markerPoint: markerHit.point,
    elevationValue: null,
    remainingSvgPoints: geometry.remainingSvgPoints,
    completedSvgPoints: pointsToSvg(completedPoints),
    areaSvgPoints: geometry.areaSvgPoints,
    completedAreaSvgPoints: areaToSvg(completedPoints, width, height, null),
  }
}
