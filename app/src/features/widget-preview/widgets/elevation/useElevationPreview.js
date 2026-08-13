import { getPreviewTextBaseline, measurePreviewText } from '../../shared/textMeasurement'
import { getTextShadowParts } from '../../shared/shadow'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'
import { useElevationPreviewGeometry } from './useElevationPreviewGeometry'
import { buildElevationPreviewStyle } from './style'
import { applyAltitudeOffset, getAltitudeCorrectionMeters } from '@/lib/widget/altitude-correction'
import { getElevationProfileSeries } from '@/features/overlay-editor'

function formatElevationLabels(elevationValue) {
  if (elevationValue === null) return { metricLabel: '-- M', imperialLabel: '-- FT' }

  return {
    metricLabel: `${Math.round(elevationValue)} M`,
    imperialLabel: `${Math.round(elevationValue * 3.28084)} FT`,
  }
}

function getElevationLabelBaseline(top, fontSize, measurement) {
  return getPreviewTextBaseline({
    top,
    lineHeight: fontSize * 0.92,
    ascent: measurement.ascent,
    glyphHeight: measurement.glyphHeight,
  })
}

/** Builds the preview model consumed by the elevation preview renderer. */
export function useElevationPreview({ widget, activity, previewSecond, globalScale, sceneStyle, exportRange }) {
  const style = buildElevationPreviewStyle(widget.data, globalScale)
  useFontMetrics([{ fontFamily: style.labelFontFamily, fontSize: widget.data.point_label.font_size }])
  const geometry = useElevationPreviewGeometry({ activity, data: widget.data, exportRange, previewSecond, style })

  if (!geometry) return null

  const labelOffset = getAltitudeCorrectionMeters(
    getElevationProfileSeries(activity),
    widget.data.starting_altitude,
    widget.data.starting_altitude_unit,
  )
  const labelAltitude = applyAltitudeOffset(geometry.elevationValue, labelOffset)
  const labels = formatElevationLabels(labelAltitude)
  const labelMeasurement = measurePreviewText(labels.metricLabel, widget.data.point_label.font_size, style.labelFontFamily)

  return {
    style,
    geometry,
    ...labels,
    metricLabelBaseline: geometry.markerPoint
      ? getElevationLabelBaseline(geometry.markerPoint[1] + widget.data.metric_label_offset_y, widget.data.point_label.font_size, labelMeasurement)
      : null,
    imperialLabelBaseline: geometry.markerPoint
      ? getElevationLabelBaseline(geometry.markerPoint[1] + widget.data.imperial_label_offset_y, widget.data.point_label.font_size, labelMeasurement)
      : null,
    shadow: getTextShadowParts(sceneStyle),
    lineShadowFilterId: sanitizeSvgId(`${widget.id}-elevation-line-shadow-blur`),
    metricLabelShadowFilterId: sanitizeSvgId(`${widget.id}-elevation-metric-label-shadow`),
    imperialLabelShadowFilterId: sanitizeSvgId(`${widget.id}-elevation-imperial-label-shadow`),
  }
}
