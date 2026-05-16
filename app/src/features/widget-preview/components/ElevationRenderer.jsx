/**
 * Renders the overlay elevation widget SVG preview — draws elevation profile
 * as an area chart with remaining/completed fill, polylines, shadow layers,
 * progress marker, and optional metric/imperial elevation labels.
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object} props.activity - Activity data with elevation series.
 * @param {number} props.previewSecond - Current preview time in seconds.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {number} props.globalScale - Global scale multiplier.
 * @param {string} props.sceneFont - Scene-level font family.
 * @param {number} props.sceneFontSize - Scene-level font size.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {string} props.valueFont - Value font family override.
 * @param {object} props.exportRange - Export range configuration.
 * @returns {JSX.Element} SVG element for elevation preview.
 */

import { useMemo } from 'react'
import { buildScopedElevationSeries, getExportWindowDistanceProgressAtElapsed, resolveExportRangeWindow } from '@/features/overlay-editor'
import { normalizeElevationGeometry } from '../utils/elevationGeometry'
import { getDistanceProgressAtElapsed, getInterpolatedSeriesValue, getSeriesValueAtProgress } from '@/features/overlay-editor'
import { getPreviewFontFamily, getPreviewTextBaseline, getWidgetOpacity, measurePreviewText } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import {
  sanitizeSvgId,
  resolveScaledPreviewLineWidth,
  resolvePreviewStyleColor,
  normalizePreviewOpacity,
  getPreviewMarkerLayers,
  buildElevationCompletedPoints,
} from '../utils/svgPreviewUtils'
import { areaToSvg, getPointAtMetricProgress, getPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { PreviewMarkerLayers, PreviewPolylineShadow, PreviewSvgShadowBlurFilter, PreviewSvgText } from './previewSvgComponents'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'

export function OverlayElevationWidget({
  widget,
  activity,
  previewSecond,
  globalOpacity,
  globalScale,
  sceneFont,
  sceneFontSize,
  sceneStyle,
  valueFont,
  exportRange,
}) {
  // Dimensions and base styling — clamp widget dimensions and resolve colors, line widths, and opacities from widget config
  const width = Math.max(widget.data.width ?? 320, 80)
  const height = Math.max(widget.data.height ?? 180, 80)
  const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)
  const baseColor = widget.data.color || '#ffffff'
  const remainingLineWidth = resolveScaledPreviewLineWidth(widget.data.remaining_line_width, widget.data.line?.width, safeGlobalScale)
  const completedLineWidth = resolveScaledPreviewLineWidth(widget.data.completed_line_width, widget.data.line?.width, safeGlobalScale)
  const remainingLineColor = resolvePreviewStyleColor(widget.data.remaining_line_color, widget.data.line?.color, baseColor)
  const completedLineColor = resolvePreviewStyleColor(widget.data.completed_line_color, widget.data.line?.color, baseColor)
  const remainingLineOpacity = normalizePreviewOpacity(widget.data.remaining_line_opacity ?? widget.data.line?.opacity ?? widget.data.opacity, 1)
  const completedLineOpacity = normalizePreviewOpacity(widget.data.completed_line_opacity ?? widget.data.line?.opacity ?? widget.data.opacity, 1)
  const markerSize = Number.isFinite(Number(widget.data.marker_size)) ? Number(widget.data.marker_size) : 16
  const svgMarkerSize = markerSize
  const markerColor = widget.data.marker_color || baseColor
  const markerOpacity = normalizePreviewOpacity(widget.data.marker_opacity ?? widget.data.opacity, 1)
  const labelFontSize = widget.data.point_label?.font_size ?? sceneFontSize ?? 12.5
  const labelFontFamily = getPreviewFontFamily(
    widget.data.point_label?.font || widget.data.point_label?.font_family || valueFont || sceneFont || widget.data.font || widget.data.font_family,
  )
  // Font metrics — trigger font loading to ensure accurate text measurement before layout
  useFontMetricsVersion(labelFontFamily, labelFontSize)

  // Export window — compute the visible time/distance range based on full activity or user crop
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, widget.data.show_full_activity ?? false),
    [activity, exportRange, widget.data.show_full_activity],
  )

  // Area styling — fill colors and opacities for the remaining (unridden) and completed (ridden) elevation area
  const remainingAreaColor = widget.data.area_remaining_color || widget.data.fill?.color || baseColor
  const completedAreaColor = widget.data.area_completed_color || widget.data.fill?.color || baseColor
  const remainingAreaOpacity = normalizePreviewOpacity(
    widget.data.area_remaining_opacity ?? (widget.data.fill?.opacity === undefined ? undefined : widget.data.fill.opacity * 0.35),
    0.12,
  )
  const completedAreaOpacity = normalizePreviewOpacity(widget.data.area_completed_opacity ?? widget.data.fill?.opacity, 0.24)

  // Elevation data — build the scoped elevation series and normalize it into SVG-space points
  const scopedElevationSeries = useMemo(() => buildScopedElevationSeries(activity, exportWindow), [activity, exportWindow])
  const profileElevations = scopedElevationSeries.values
  const profileDistanceProgress = scopedElevationSeries.progressValues
  const elevationGeometry = useMemo(() => {
    // Normalize elevation data at full resolution, then divide back by scale for unscaled preview coords
    const scaledGeometry = normalizeElevationGeometry(
      profileElevations,
      width * safeGlobalScale,
      height * safeGlobalScale,
      widget.data.margin ?? 0,
      widget.data.y_scale ?? 1,
      profileDistanceProgress,
      widget.data.target_density ?? 0.75,
      widget.data.simplify_tolerance_px ?? 1,
    )

    return {
      ...scaledGeometry,
      points: scaledGeometry.points.map(([x, y]) => [x / safeGlobalScale, y / safeGlobalScale]),
    }
  }, [
    height,
    profileDistanceProgress,
    profileElevations,
    safeGlobalScale,
    width,
    widget.data.margin,
    widget.data.simplify_tolerance_px,
    widget.data.target_density,
    widget.data.y_scale,
  ])
  const points = elevationGeometry.points
  const pointProgress = elevationGeometry.progressValues

  // Playhead position — compute 0–1 progress and locate the marker point on the elevation profile
  const progress01 = exportWindow.active
    ? (getExportWindowDistanceProgressAtElapsed(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)
  const markerPoint =
    getPointAtMetricProgress(points, pointProgress, progress01) || getPointAtProgress(points, progress01) || points[points.length - 1]
  const completedPoints = useMemo(
    () => buildElevationCompletedPoints(points, pointProgress, progress01, markerPoint),
    [markerPoint, pointProgress, points, progress01],
  )

  // Elevation value at playhead — interpolate the elevation at the current progress position
  const elevationValue =
    getInterpolatedSeriesValue(profileDistanceProgress, profileElevations, progress01) ?? getSeriesValueAtProgress(profileElevations, progress01)

  // SVG paths — convert remaining/completed point arrays into SVG point string formats
  const areaSvgPoints = useMemo(() => areaToSvg(points, width, height, null), [height, points, width])
  const completedAreaSvgPoints = useMemo(() => areaToSvg(completedPoints, width, height, null), [completedPoints, height, width])
  const remainingSvgPoints = pointsToSvg(points)
  const completedSvgPoints = pointsToSvg(completedPoints)

  // Elevation labels — build metric ("M") and imperial ("FT") label text from the interpolated elevation value
  const metricLabel = elevationValue === null || elevationValue === undefined ? '-- M' : `${Math.round(elevationValue)} M`
  const imperialLabel = elevationValue === null || elevationValue === undefined ? '-- FT' : `${Math.round(elevationValue * 3.28084)} FT`

  // Marker layers and label positioning — build concentric circle layers and measure label text for baseline centering
  const markerLayers = useMemo(
    () => getPreviewMarkerLayers(widget.data, svgMarkerSize, markerColor, markerOpacity),
    [markerColor, markerOpacity, svgMarkerSize, widget.data],
  )
  const labelMeasurement = measurePreviewText(metricLabel, labelFontSize, labelFontFamily)
  const getElevationLabelBaseline = (top) =>
    getPreviewTextBaseline({
      top,
      lineHeight: labelFontSize * 0.92,
      ascent: labelMeasurement.ascent,
      descent: labelMeasurement.descent,
      glyphHeight: labelMeasurement.glyphHeight,
    })
  const labelColor = widget.data.point_label?.color || baseColor
  const showMetricLabel = widget.data.show_elevation_metric ?? false
  const showImperialLabel = widget.data.show_elevation_imperial ?? false
  const shadow = getTextShadowParts(sceneStyle)
  const lineShadowFilterId = sanitizeSvgId(`${widget.id}-elevation-line-shadow-blur`)

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className="block h-full w-full overflow-visible"
      style={{ opacity: getWidgetOpacity(widget.data, globalOpacity) }}
    >
      <PreviewSvgShadowBlurFilter id={lineShadowFilterId} shadow={shadow} />
      <polygon points={areaSvgPoints} fill={remainingAreaColor} fillOpacity={remainingAreaOpacity} />
      <PreviewPolylineShadow
        points={remainingSvgPoints}
        shadow={shadow}
        blurFilterId={lineShadowFilterId}
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
      <polygon points={completedAreaSvgPoints} fill={completedAreaColor} fillOpacity={completedAreaOpacity} />
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
      {markerPoint && showMetricLabel ? (
        <PreviewSvgText
          text={metricLabel}
          x={markerPoint[0] + (widget.data.metric_label_offset_x ?? 0)}
          baseline={getElevationLabelBaseline(markerPoint[1] + (widget.data.metric_label_offset_y ?? -28))}
          color={labelColor}
          fontFamily={labelFontFamily}
          fontSize={labelFontSize}
          opacity={1}
          shadow={shadow}
          shadowFilterId={sanitizeSvgId(`${widget.id}-elevation-metric-label-shadow`)}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
      {markerPoint && showImperialLabel ? (
        <PreviewSvgText
          text={imperialLabel}
          x={markerPoint[0] + (widget.data.imperial_label_offset_x ?? 0)}
          baseline={getElevationLabelBaseline(markerPoint[1] + (widget.data.imperial_label_offset_y ?? 6))}
          color={labelColor}
          fontFamily={labelFontFamily}
          fontSize={labelFontSize}
          opacity={1}
          shadow={shadow}
          shadowFilterId={sanitizeSvgId(`${widget.id}-elevation-imperial-label-shadow`)}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
    </svg>
  )
}
