/**
 * Renders the overlay elevation widget SVG preview — draws elevation profile
 * as an area chart with remaining/completed fill, polylines, shadow layers,
 * progress marker, and optional metric/imperial elevation labels.
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object|null} props.activity - Stable parsed activity used to prepare geometry and display activity data.
 * @param {number} props.previewSecond - Current preview time in seconds.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {number} props.globalScale - Global scale multiplier.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {object} props.exportRange - Export range configuration.
 * @returns {JSX.Element} SVG element for elevation preview.
 */

import { getWidgetOpacity } from '../../shared/textMeasurement'
import { PreviewMarkerLayers, PreviewPolylineShadow, PreviewSvgShadowBlurFilter, PreviewSvgText } from '../../shared/PreviewSvgComponents'
import { useElevationPreview } from './useElevationPreview'

export function OverlayElevationWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle, exportRange }) {
  const previewModel = useElevationPreview({ widget, activity, previewSecond, globalScale, sceneStyle, exportRange })

  if (!previewModel) return null

  const markerPoint = previewModel.geometry.markerPoint

  return (
    <svg
      width={widget.data.width}
      height={widget.data.height}
      viewBox={`0 0 ${widget.data.width} ${widget.data.height}`}
      preserveAspectRatio="none"
      className="block h-full w-full overflow-visible"
      style={{ opacity: getWidgetOpacity(widget.data, globalOpacity) }}
    >
      <PreviewSvgShadowBlurFilter id={previewModel.lineShadowFilterId} shadow={previewModel.shadow} />
      <g>
        <polygon
          points={previewModel.geometry.areaSvgPoints}
          fill={widget.data.area_remaining_color}
          fillOpacity={previewModel.style.remainingAreaOpacity}
        />
        <PreviewPolylineShadow
          points={previewModel.geometry.remainingSvgPoints}
          shadow={previewModel.shadow}
          blurFilterId={previewModel.lineShadowFilterId}
          strokeWidth={widget.data.remaining_line_width}
          strokeOpacity={previewModel.style.remainingLineOpacity}
          rotation={widget.data.rotation}
        />
        <polyline
          fill="none"
          stroke={widget.data.remaining_line_color}
          strokeOpacity={previewModel.style.remainingLineOpacity}
          strokeWidth={widget.data.remaining_line_width}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={previewModel.geometry.remainingSvgPoints}
        />
        <polygon
          points={previewModel.geometry.completedAreaSvgPoints}
          fill={widget.data.area_completed_color}
          fillOpacity={previewModel.style.completedAreaOpacity}
        />
        <polyline
          fill="none"
          stroke={widget.data.completed_line_color}
          strokeOpacity={previewModel.style.completedLineOpacity}
          strokeWidth={widget.data.completed_line_width}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={previewModel.geometry.completedSvgPoints}
        />
      </g>
      <PreviewMarkerLayers layers={previewModel.style.markerLayers} point={markerPoint} />
      {markerPoint && widget.data.show_elevation_metric ? (
        <PreviewSvgText
          text={previewModel.metricLabel}
          x={markerPoint[0] + widget.data.metric_label_offset_x}
          baseline={previewModel.metricLabelBaseline}
          color={widget.data.point_label.color}
          fontFamily={previewModel.style.labelFontFamily}
          fontSize={widget.data.point_label.font_size}
          opacity={1}
          shadow={previewModel.shadow}
          shadowFilterId={previewModel.metricLabelShadowFilterId}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
      {markerPoint && widget.data.show_elevation_imperial ? (
        <PreviewSvgText
          text={previewModel.imperialLabel}
          x={markerPoint[0] + widget.data.imperial_label_offset_x}
          baseline={previewModel.imperialLabelBaseline}
          color={widget.data.point_label.color}
          fontFamily={previewModel.style.labelFontFamily}
          fontSize={widget.data.point_label.font_size}
          opacity={1}
          shadow={previewModel.shadow}
          shadowFilterId={previewModel.imperialLabelShadowFilterId}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
    </svg>
  )
}
