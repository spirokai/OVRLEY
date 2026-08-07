import { useMemo } from 'react'
import { getDistanceProgressAtElapsed, getPreviewActivity, getWindowProgressAtTime } from '@/features/overlay-editor'
import { buildElevationGeometry } from '@/api/backend'
import { areaToSvg, findPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { buildPlaceholderElevationPreviewGeometry, buildPlaceholderElevationStaticGeometry } from '../../shared/plotPlaceholderGeometry'
import { buildElevationCompletedPoints } from '../../shared/svgPreviewUtils'
import { interpolateNumericSeries } from '@/lib/interpolation'
import { usePlotPreviewGeometry } from '../../shared/usePlotPreviewGeometry'

function projectElevationValueToSvgY(elevationValue, dataRange, height, yScale) {
  if (elevationValue === null) return null

  const [minElevation, maxElevation] = dataRange
  const span = Math.max(maxElevation - minElevation, 1e-9)
  const normalized = (elevationValue - minElevation) / span
  const centered = Math.min(Math.max((normalized - 0.5) * yScale + 0.5, 0), 1)

  return height - height * centered
}

/**
 * Builds the geometry model for the elevation preview renderer.
 *
 * Rust handles the expensive geometry pipeline (smoothing, downsampling,
 * projection, RDP simplification) via IPC. This hook consumes the result
 * and performs cheap per-frame operations locally (marker interpolation,
 * completed polyline, SVG paths) that must run at 30fps.
 *
 * For canvas-parity testing, window.__OVRLEY_MOCK_ELEVATION_GEOMETRY
 * injects pre-computed Rust geometry so Skia and SVG use identical data.
 *
 * @param {object} params
 * @param {object|null} params.activity - Stable parsed activity used to prepare geometry and display activity data.
 * @param {object} params.data - Effective elevation widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by buildElevationPreviewStyle.
 * @returns {object|null} Geometry model for the renderer, or null while loading.
 */
export function useElevationPreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  const { areaSvgPoints, exportWindow, fallbackDurationSeconds, points, remainingSvgPoints, rustGeometry } = usePlotPreviewGeometry({
    activity,
    data,
    exportRange,
    style,
    plotType: 'elevation',
    buildGeometry: buildElevationGeometry,
    mockGeometryKey: '__OVRLEY_MOCK_ELEVATION_GEOMETRY',
    includeArea: true,
  })
  const isPlaceholder = getPreviewActivity(activity, previewSecond) === null
  const placeholderStaticGeometry = useMemo(
    () => (isPlaceholder ? buildPlaceholderElevationStaticGeometry({ width: data.width, height: data.height }) : null),
    [data.height, data.width, isPlaceholder],
  )

  if (isPlaceholder) {
    return buildPlaceholderElevationPreviewGeometry({
      width: data.width,
      height: data.height,
      previewSecond,
      fallbackDurationSeconds,
      staticGeometry: placeholderStaticGeometry,
    })
  }

  if (!rustGeometry) return null

  // Keep marker x distance-based so it stays put during hover/stop segments.
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  // Completed profile fill is chronological, normalized to the same scoped duration
  // Rust used when building elapsedFractions.
  const sourceDuration = exportWindow.active ? exportWindow.end - exportWindow.start : activity.sample_elapsed_seconds.at(-1) || 1
  const elapsedWindowStart = exportWindow.active ? exportWindow.start : 0
  const frameElapsedFraction = Math.min(Math.max((previewSecond - elapsedWindowStart) / Math.max(sourceDuration, 1e-9), 0), 1)

  const metricHit = findPointAtProgress(points, rustGeometry.progressValues, progress01)
  const elevationSeries = activity.sample_elevations.length ? activity.sample_elevations : activity.elevation
  const elevationValue = interpolateNumericSeries(activity.sample_elapsed_seconds, elevationSeries, previewSecond)
  const markerY = projectElevationValueToSvgY(elevationValue, rustGeometry.dataRange, data.height, data.y_scale)
  const markerPoint = markerY === null ? null : [metricHit.point[0], markerY]
  const completedPoints = buildElevationCompletedPoints(
    points,
    rustGeometry.progressValues,
    rustGeometry.elapsedFractions,
    progress01,
    frameElapsedFraction,
  )

  return {
    markerPoint,
    elevationValue,
    remainingSvgPoints,
    completedSvgPoints: pointsToSvg(completedPoints),
    areaSvgPoints,
    completedAreaSvgPoints: areaToSvg(completedPoints, data.width, data.height, null),
  }
}
