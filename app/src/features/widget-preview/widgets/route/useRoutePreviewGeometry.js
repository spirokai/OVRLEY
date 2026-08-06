import { useMemo } from 'react'
import { getDistanceProgressAtElapsed, getPreviewActivity, getWindowProgressAtTime } from '@/features/overlay-editor'
import { buildRouteGeometry } from '@/api/backend'
import { pointsToSvg } from '@/lib/geometryUtils'
import { buildPlaceholderRoutePreviewGeometry, buildPlaceholderRouteStaticGeometry } from '../../shared/plotPlaceholderGeometry'
import { buildRouteFramePreview } from '../../shared/svgPreviewUtils'
import { usePlotPreviewGeometry } from '../../shared/usePlotPreviewGeometry'

/**
 * Builds the geometry model for the route preview renderer.
 *
 * Rust handles the expensive geometry pipeline (Mercator projection,
 * LTTB downsampling, RDP simplification, widget fitting) via IPC.
 * This hook consumes the result and performs cheap per-frame operations
 * locally (marker interpolation, completed segment, SVG paths) that
 * must run at 30fps.
 *
 * For canvas-parity testing, window.__OVRLEY_MOCK_ROUTE_GEOMETRY
 * injects pre-computed Rust geometry so Skia and SVG use identical data.
 *
 * @param {object} params
 * @param {object|null} params.activity - Stable parsed activity used to prepare geometry and display activity data.
 * @param {object} params.data - Effective route widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by useRoutePreviewStyle.
 * @returns {object|null} Geometry model for the renderer, or null while loading.
 */
export function useRoutePreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  const { exportWindow, fallbackDurationSeconds, points, remainingSvgPoints, rustGeometry } = usePlotPreviewGeometry({
    activity,
    data,
    exportRange,
    style,
    plotType: 'course',
    buildGeometry: buildRouteGeometry,
    mockGeometryKey: '__OVRLEY_MOCK_ROUTE_GEOMETRY',
  })
  const isPlaceholder = getPreviewActivity(activity, previewSecond) === null
  const placeholderStaticGeometry = useMemo(
    () => (isPlaceholder ? buildPlaceholderRouteStaticGeometry({ width: data.width, height: data.height }) : null),
    [data.height, data.width, isPlaceholder],
  )

  if (isPlaceholder) {
    return buildPlaceholderRoutePreviewGeometry({
      width: data.width,
      height: data.height,
      previewSecond,
      fallbackDurationSeconds,
      staticGeometry: placeholderStaticGeometry,
    })
  }

  if (!rustGeometry) return null

  // progress01 drives marker placement and completed polyline. Export
  // window normalizes it to 0..1 within the trimmed range.
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  const { markerPoint, completedPoints } = buildRouteFramePreview(points, rustGeometry.progressValues, progress01)

  return {
    markerPoint,
    remainingSvgPoints,
    completedSvgPoints: pointsToSvg(completedPoints),
  }
}
