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
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {object} props.exportRange - Export range configuration.
 * @returns {JSX.Element} SVG element for elevation preview.
 */

import { getPreviewTextBaseline, getWidgetOpacity, measurePreviewText } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { sanitizeSvgId, getPreviewMarkerLayers } from '../utils/svgPreviewUtils'
import { PreviewMarkerLayers, PreviewPolylineShadow, PreviewSvgShadowBlurFilter, PreviewSvgText } from './previewSvgComponents'
import { useElevationPreviewStyle } from '../hooks/useElevationPreviewStyle'
import { useElevationPreviewGeometry } from '../hooks/useElevationPreviewGeometry'

export function OverlayElevationWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle, exportRange }) {
  const data = widget.data
  const style = useElevationPreviewStyle(data, globalScale)
  const geometry = useElevationPreviewGeometry({ activity, data, exportRange, previewSecond, style })

  if (!geometry) {
    return null
  }

  // Elevation labels — build metric ("M") and imperial ("FT") label text from the interpolated elevation value
  const metricLabel = geometry.elevationValue === null || geometry.elevationValue === undefined ? '-- M' : `${Math.round(geometry.elevationValue)} M`
  const imperialLabel =
    geometry.elevationValue === null || geometry.elevationValue === undefined ? '-- FT' : `${Math.round(geometry.elevationValue * 3.28084)} FT`

  // Marker layers and label positioning — build concentric circle layers and measure label text for baseline centering
  const markerLayers = getPreviewMarkerLayers(data, style.markerSize, style.markerColor, style.markerOpacity)
  const labelMeasurement = measurePreviewText(metricLabel, style.labelFontSize, style.labelFontFamily)
  const getElevationLabelBaseline = (top) =>
    getPreviewTextBaseline({
      top,
      lineHeight: style.labelFontSize * 0.92,
      ascent: labelMeasurement.ascent,
      descent: labelMeasurement.descent,
      glyphHeight: labelMeasurement.glyphHeight,
    })
  const shadow = getTextShadowParts(sceneStyle)
  const lineShadowFilterId = sanitizeSvgId(`${widget.id}-elevation-line-shadow-blur`)

  return (
    <svg
      width={style.width}
      height={style.height}
      viewBox={`0 0 ${style.width} ${style.height}`}
      preserveAspectRatio="none"
      className="block h-full w-full overflow-visible"
      style={{ opacity: getWidgetOpacity(data, globalOpacity) }}
    >
      <PreviewSvgShadowBlurFilter id={lineShadowFilterId} shadow={shadow} />
      <polygon points={geometry.areaSvgPoints} fill={data.area_remaining_color} fillOpacity={style.remainingAreaOpacity} />
      <PreviewPolylineShadow
        points={geometry.remainingSvgPoints}
        shadow={shadow}
        blurFilterId={lineShadowFilterId}
        strokeWidth={style.remainingLineWidth}
        strokeOpacity={style.remainingLineOpacity}
        rotation={data.rotation ?? 0}
      />
      <polyline
        fill="none"
        stroke={data.remaining_line_color}
        strokeOpacity={style.remainingLineOpacity}
        strokeWidth={style.remainingLineWidth}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={geometry.remainingSvgPoints}
      />
      <polygon points={geometry.completedAreaSvgPoints} fill={data.area_completed_color} fillOpacity={style.completedAreaOpacity} />
      <polyline
        fill="none"
        stroke={data.completed_line_color}
        strokeOpacity={style.completedLineOpacity}
        strokeWidth={style.completedLineWidth}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={geometry.completedSvgPoints}
      />
      <PreviewMarkerLayers layers={markerLayers} x={geometry.markerPoint?.[0]} y={geometry.markerPoint?.[1]} />
      {geometry.markerPoint && style.showMetricLabel ? (
        <PreviewSvgText
          text={metricLabel}
          x={geometry.markerPoint[0] + data.metric_label_offset_x}
          baseline={getElevationLabelBaseline(geometry.markerPoint[1] + data.metric_label_offset_y)}
          color={style.labelColor}
          fontFamily={style.labelFontFamily}
          fontSize={style.labelFontSize}
          opacity={1}
          shadow={shadow}
          shadowFilterId={sanitizeSvgId(`${widget.id}-elevation-metric-label-shadow`)}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
      {geometry.markerPoint && style.showImperialLabel ? (
        <PreviewSvgText
          text={imperialLabel}
          x={geometry.markerPoint[0] + data.imperial_label_offset_x}
          baseline={getElevationLabelBaseline(geometry.markerPoint[1] + data.imperial_label_offset_y)}
          color={style.labelColor}
          fontFamily={style.labelFontFamily}
          fontSize={style.labelFontSize}
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
