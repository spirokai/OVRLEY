/**
 * Renders the overlay route widget SVG preview — draws remaining/completed
 * polylines, shadow layers, and marker indicators for the course route.
 *
 * All data is received via props; no store access.
 */

import { useMemo } from 'react'
import { buildScopedRouteSamples, getExportWindowDistanceProgressAtElapsed, resolveExportRangeWindow } from '@/features/overlay-editor'
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
  const svgMarkerSize = markerSize
  const markerColor = widget.data.marker_color || baseColor
  const markerOpacity = normalizePreviewOpacity(widget.data.marker_opacity ?? widget.data.opacity, 1)
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, widget.data.show_full_activity ?? false),
    [activity, exportRange, widget.data.show_full_activity],
  )
  const routeSamples = useMemo(() => {
    return buildScopedRouteSamples(activity, exportWindow)
  }, [activity, exportWindow])
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
        markerSize,
      ),
    [
      geometryCompletedLineWidth,
      geometryRemainingLineWidth,
      height,
      markerSize,
      routeSamples,
      width,
      widget.data.simplify_tolerance_px,
      widget.data.target_density,
    ],
  )
  const pointProgress = routeGeometry.progressValues
  const progress01 = exportWindow.active
    ? (getExportWindowDistanceProgressAtElapsed(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)
  const { markerPoint, completedPoints } = useMemo(
    () => buildRouteFramePreview(routeGeometry.points, pointProgress, progress01),
    [pointProgress, progress01, routeGeometry.points],
  )
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
