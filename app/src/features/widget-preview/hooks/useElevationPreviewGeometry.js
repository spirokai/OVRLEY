import { useMemo } from 'react'
import {
  buildScopedElevationSeries,
  getDistanceProgressAtElapsed,
  getInterpolatedSeriesValue,
  getSeriesValueAtProgress,
  getWindowProgressAtTime,
  resolveExportRangeWindow,
} from '@/features/overlay-editor'
import { areaToSvg, getPointAtMetricProgress, getPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { normalizeElevationGeometry } from '../utils/elevationGeometry'
import { buildElevationCompletedPoints } from '../utils/svgPreviewUtils'

/**
 * Builds the geometry model for the elevation preview renderer.
 *
 * Given effective widget data plus the style model, this hook resolves the
 * active export window, scopes the activity elevation series to that window,
 * normalizes the series into SVG coordinates, computes playhead progress and
 * marker position, and materializes the remaining/completed SVG paths used by
 * the renderer.
 *
 * Stages:
 * 1. Resolve the active export window and scoped elevation series.
 * 2. Normalize elevation samples into SVG-space geometry.
 * 3. Compute current progress, marker position, and completed path.
 * 4. Convert point arrays into SVG-ready path inputs.
 *
 * @param {object} params - Geometry inputs for the preview frame.
 * @param {object} params.activity - Activity data with elevation samples.
 * @param {object} params.data - Effective elevation widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by useElevationPreviewStyle.
 * @returns {object} Geometry model consumed by the elevation preview renderer.
 */
export function useElevationPreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  // Export scoping: pick the visible portion of the activity for this preview.
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, data.show_full_activity),
    [activity, exportRange, data.show_full_activity],
  )

  // Series prep: build the elevation/value arrays used for interpolation and geometry.
  const scopedElevationSeries = useMemo(() => buildScopedElevationSeries(activity, exportWindow), [activity, exportWindow])
  const profileElevations = scopedElevationSeries.values
  const profileDistanceProgress = scopedElevationSeries.progressValues

  const elevationGeometry = useMemo(() => {
    // Normalize at scaled resolution, then divide back to unscaled SVG coordinates.
    const scaledGeometry = normalizeElevationGeometry(
      profileElevations,
      style.width * style.safeGlobalScale,
      style.height * style.safeGlobalScale,
      data.margin ?? 0,
      data.y_scale,
      profileDistanceProgress,
      data.target_density,
      data.simplify_tolerance_px,
    )

    return {
      ...scaledGeometry,
      points: scaledGeometry.points.map(([x, y]) => [x / style.safeGlobalScale, y / style.safeGlobalScale]),
    }
  }, [
    data.margin,
    data.simplify_tolerance_px,
    data.target_density,
    data.y_scale,
    profileDistanceProgress,
    profileElevations,
    style.height,
    style.safeGlobalScale,
    style.width,
  ])

  // Playhead progress: prefer export-window progress when cropping is active.
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  // Marker placement: locate the best point on the simplified preview geometry.
  const points = elevationGeometry.points
  const pointProgress = elevationGeometry.progressValues
  const markerPoint =
    getPointAtMetricProgress(points, pointProgress, progress01) || getPointAtProgress(points, progress01) || points[points.length - 1]

  // Completed path: keep only the ridden/elapsed portion of the profile.
  const completedPoints = useMemo(
    () => buildElevationCompletedPoints(points, pointProgress, progress01, markerPoint),
    [markerPoint, pointProgress, points, progress01],
  )

  // Value interpolation: sample the elevation series at the current progress.
  const elevationValue =
    getInterpolatedSeriesValue(profileDistanceProgress, profileElevations, progress01) ?? getSeriesValueAtProgress(profileElevations, progress01)

  return useMemo(
    // SVG conversion: materialize the point arrays the renderer draws directly.
    () => ({
      markerPoint,
      elevationValue,
      remainingSvgPoints: pointsToSvg(points),
      completedSvgPoints: pointsToSvg(completedPoints),
      areaSvgPoints: areaToSvg(points, style.width, style.height, null),
      completedAreaSvgPoints: areaToSvg(completedPoints, style.width, style.height, null),
    }),
    [completedPoints, elevationValue, markerPoint, points, style.height, style.width],
  )
}
