import { useMemo } from 'react'
import {
  buildExportWindowRouteSamples,
  getDistanceProgressAtElapsed,
  getWindowProgressAtTime,
  resolveExportRangeWindow,
} from '@/features/overlay-editor'
import { pointsToSvg } from '@/lib/geometryUtils'
import { normalizeRouteGeometry } from '../utils/routeGeometry'
import { buildRouteFramePreview } from '../utils/svgPreviewUtils'

/**
 * Builds the geometry model for the route preview renderer.
 *
 * Resolves the active export window, scopes route samples to that window,
 * normalizes the route into SVG coordinates, computes playhead progress, and
 * materializes the remaining/completed SVG paths plus marker position.
 *
 * Stages:
 * 1. Resolve the active export window and scoped route samples.
 * 2. Normalize route samples into preview geometry.
 * 3. Compute current progress, marker position, and completed segment.
 * 4. Convert point arrays into SVG-ready point strings.
 *
 * @param {object} params - Geometry inputs for the preview frame.
 * @param {object} params.activity - Activity data with route samples.
 * @param {object} params.data - Effective route widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by useRoutePreviewStyle.
 * @returns {object} Geometry model consumed by the route preview renderer.
 */
export function useRoutePreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  // Export scoping: pick the visible activity range for the route preview.
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, data.show_full_activity),
    [activity, exportRange, data.show_full_activity],
  )

  // Sample extraction: collect only the route samples inside the active window.
  const routeSamples = useMemo(() => buildExportWindowRouteSamples(activity, exportWindow), [activity, exportWindow])

  const routeGeometry = useMemo(
    // Geometry normalization: project route samples and simplify for SVG rendering.
    () =>
      normalizeRouteGeometry(
        routeSamples,
        style.width,
        style.height,
        data.target_density,
        data.simplify_tolerance_px,
        style.geometryRemainingLineWidth,
        style.geometryCompletedLineWidth,
        style.routeMarkerInsetRadius,
      ),
    [
      data.simplify_tolerance_px,
      data.target_density,
      routeSamples,
      style.geometryCompletedLineWidth,
      style.geometryRemainingLineWidth,
      style.height,
      style.routeMarkerInsetRadius,
      style.width,
    ],
  )

  // Playhead progress: resolve progress against the crop window when active.
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  const pointProgress = routeGeometry.progressValues
  // Marker placement: derive both the marker point and the completed route segment.
  const { markerPoint, completedPoints } = useMemo(
    () => buildRouteFramePreview(routeGeometry.points, pointProgress, progress01),
    [pointProgress, progress01, routeGeometry.points],
  )

  return useMemo(
    // SVG conversion: materialize polyline point strings the renderer can draw directly.
    () => ({
      markerPoint,
      remainingSvgPoints: pointsToSvg(routeGeometry.points),
      completedSvgPoints: pointsToSvg(completedPoints),
    }),
    [completedPoints, markerPoint, routeGeometry.points],
  )
}
