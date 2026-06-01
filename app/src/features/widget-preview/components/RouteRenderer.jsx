/**
 * Renders the overlay route widget SVG preview — draws remaining/completed
 * polylines, shadow layers, and marker indicators for the course route.
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object} props.activity - Activity data with route samples.
 * @param {number} props.previewSecond - Current preview time in seconds.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {number} props.globalScale - Global scale multiplier.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {object} props.exportRange - Export range configuration.
 * @returns {JSX.Element} SVG element for route preview.
 */

import { useMemo } from 'react'
import { buildExportWindowRouteSamples, getWindowProgressAtTime, resolveExportRangeWindow } from '@/features/overlay-editor'
import { normalizeRouteGeometry } from '../utils/routeGeometry'
import { getDistanceProgressAtElapsed } from '@/features/overlay-editor'
import { getWidgetOpacity } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import {
  sanitizeSvgId,
  resolvePreviewLineWidth,
  resolveScaledPreviewLineWidth,
  resolvePreviewStyleColor,
  normalizePreviewOpacity,
  getPreviewMarkerLayers,
  buildRouteFramePreview,
} from '../utils/svgPreviewUtils'
import { pointsToSvg } from '@/lib/geometryUtils'
import { PreviewMarkerLayers, PreviewPolylineShadow, PreviewSvgShadowBlurFilter } from './previewSvgComponents'

export function OverlayRouteWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle, exportRange }) {
  // Dimensions and base styling — clamp widget size and resolve line colors, widths, and opacities from widget config
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)
  const baseColor = widget.data.color || '#ffffff'
  const geometryRemainingLineWidth = resolvePreviewLineWidth(widget.data.remaining_line_width, widget.data.line?.width)
  const geometryCompletedLineWidth = resolvePreviewLineWidth(widget.data.completed_line_width, widget.data.line?.width)
  const remainingLineWidth = resolveScaledPreviewLineWidth(widget.data.remaining_line_width, widget.data.line?.width, safeGlobalScale)
  const completedLineWidth = resolveScaledPreviewLineWidth(widget.data.completed_line_width, widget.data.line?.width, safeGlobalScale)
  const remainingLineColor = resolvePreviewStyleColor(widget.data.remaining_line_color, widget.data.line?.color, baseColor)
  const completedLineColor = resolvePreviewStyleColor(widget.data.completed_line_color, widget.data.line?.color, baseColor)
  const remainingLineOpacity = normalizePreviewOpacity(widget.data.remaining_line_opacity ?? widget.data.line?.opacity ?? widget.data.opacity, 0.75)
  const completedLineOpacity = normalizePreviewOpacity(widget.data.completed_line_opacity ?? widget.data.line?.opacity ?? widget.data.opacity, 1)
  const markerSize = Number.isFinite(Number(widget.data.marker_size)) ? Number(widget.data.marker_size) : 18
  const markerVariantDiameter =
    Number.isFinite(Number(widget.data.marker_variant_diameter)) && Number(widget.data.marker_variant_diameter) >= 0
      ? Number(widget.data.marker_variant_diameter)
      : Math.max(markerSize * 2 + 8, 8)
  const routeMarkerInsetRadius = Math.max(markerSize, markerVariantDiameter * 0.5)
  const svgMarkerSize = markerSize
  const markerColor = widget.data.marker_color || baseColor
  const markerOpacity = normalizePreviewOpacity(widget.data.marker_opacity ?? widget.data.opacity, 1)

  // Export window and route samples — compute the visible range and fetch scoped route data
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, widget.data.show_full_activity ?? false),
    [activity, exportRange, widget.data.show_full_activity],
  )
  const routeSamples = useMemo(() => {
    return buildExportWindowRouteSamples(activity, exportWindow)
  }, [activity, exportWindow])

  // Route geometry — project lat/lng samples to SVG points with Mercator, downsample, and simplify
  const routeGeometry = useMemo(
    () =>
      normalizeRouteGeometry(
        routeSamples,
        width,
        height,
        widget.data.target_density ?? 1,
        widget.data.simplify_tolerance_px ?? 1,
        geometryRemainingLineWidth,
        geometryCompletedLineWidth,
        routeMarkerInsetRadius,
      ),
    [
      geometryCompletedLineWidth,
      geometryRemainingLineWidth,
      height,
      routeMarkerInsetRadius,
      routeSamples,
      width,
      widget.data.simplify_tolerance_px,
      widget.data.target_density,
    ],
  )
  const pointProgress = routeGeometry.progressValues

  // Playhead position — compute 0–1 progress and determine marker + completed route segment
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)
  const { markerPoint, completedPoints } = useMemo(
    () => buildRouteFramePreview(routeGeometry.points, pointProgress, progress01),
    [pointProgress, progress01, routeGeometry.points],
  )

  // SVG paths — convert remaining/completed point arrays to SVG point strings and build marker layers
  const remainingSvgPoints = useMemo(() => pointsToSvg(routeGeometry.points), [routeGeometry.points])
  const completedSvgPoints = useMemo(() => pointsToSvg(completedPoints), [completedPoints])
  const markerLayers = useMemo(
    () => getPreviewMarkerLayers(widget.data, svgMarkerSize, markerColor, markerOpacity),
    [markerColor, markerOpacity, svgMarkerSize, widget.data],
  )
  const shadow = getTextShadowParts(sceneStyle)
  const shadowFilterId = sanitizeSvgId(`${widget.id}-route-shadow-blur`)

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block h-full w-full"
      style={{ opacity: getWidgetOpacity(widget.data, globalOpacity) }}
    >
      <PreviewSvgShadowBlurFilter id={shadowFilterId} shadow={shadow} />
      <g>
        <PreviewPolylineShadow
          points={remainingSvgPoints}
          shadow={shadow}
          blurFilterId={shadowFilterId}
          strokeWidth={remainingLineWidth}
          strokeOpacity={remainingLineOpacity}
          rotation={widget.data.rotation ?? 0}
        />
        <polyline
          fill="none"
          stroke={remainingLineColor}
          strokeOpacity={remainingLineOpacity}
          strokeWidth={remainingLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={remainingSvgPoints}
        />
        <polyline
          fill="none"
          stroke={completedLineColor}
          strokeOpacity={completedLineOpacity}
          strokeWidth={completedLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={completedSvgPoints}
        />
        <PreviewMarkerLayers layers={markerLayers} x={markerPoint?.[0]} y={markerPoint?.[1]} />
      </g>
    </svg>
  )
}
