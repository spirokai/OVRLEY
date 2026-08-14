import { useMemo } from 'react'
import { getPreviewActivity, resolveMetricPresentationValues } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { getArcGaugeLayout, getArcLabelGap, getCornerGaugeLayout } from './geometry'
import { getArcInnerWidgetLayout } from './arcGaugeInnerLayout'
import { getArcFilledTrackRevealSpec, getArcPoint } from './trackPath'
import { getArcBarSegments, getBarFillCount } from '../../shared/gaugeBarGeometry'
import { buildArcGaugeInnerWidgetModel } from '../metric/model'
import { getTextShadowParts } from '../../shared/shadow'
import { getPreviewFontFamily, measureArcPreviewText } from '../../shared/textMeasurement'
import { useFontMetrics } from '../../shared/useFontMetrics'
import { formatGaugeBoundaryLabel } from '../../shared/gaugeLabelFormat'

/** Returns the SVG text origin that centers measured text around an x-coordinate. */
function centeredTextX(measurement, centerX) {
  return centerX - (measurement.boundsLeft + measurement.boundsRight) * 0.5
}

/** Returns the SVG baseline that centers measured text around a y-coordinate. */
function centeredTextBaseline(measurement, centerY) {
  return centerY + (measurement.ascent - measurement.descent) * 0.5
}

/** Returns measured min/max label positions for an arc layout. */
function getLabelLayout(layout, minLabel, maxLabel, fontFamily, fontSize) {
  const minMeasurement = measureArcPreviewText(minLabel, fontSize, fontFamily)
  const maxMeasurement = measureArcPreviewText(maxLabel, fontSize, fontFamily)
  const labelRadius = layout.radius + layout.trackThickness * 0.5 + layout.borderThickness + getArcLabelGap(fontSize)
  const minAnchor = getArcPoint(layout.centerX, layout.centerY, labelRadius, layout.labelAngles.min)
  const maxAnchor = getArcPoint(layout.centerX, layout.centerY, labelRadius, layout.labelAngles.max)

  return {
    min: { x: centeredTextX(minMeasurement, minAnchor.x), baseline: centeredTextBaseline(minMeasurement, minAnchor.y) },
    max: { x: centeredTextX(maxMeasurement, maxAnchor.x), baseline: centeredTextBaseline(maxMeasurement, maxAnchor.y) },
  }
}

/**
 * Builds all non-JSX presentation state for an arc or corner gauge.
 * @param {object} params - Normalized widget and live preview inputs.
 * @returns {object} Presentation model consumed by the gauge renderer.
 */
export function useArcGaugePreviewPresentation({ widget, activity, previewSecond, globalOpacity, sceneStyle }) {
  const valueFontFamily = getPreviewFontFamily(widget.data.font)
  const labelFontFamily = getPreviewFontFamily(widget.data.min_max_label_font)
  useFontMetrics([
    { fontFamily: valueFontFamily, fontSize: widget.data.font_size },
    { fontFamily: labelFontFamily, fontSize: widget.data.min_max_label_font_size },
  ])

  return useMemo(() => {
    const displayActivity = getPreviewActivity(activity, previewSecond)
    const presentation = resolveMetricPresentationValues(widget, displayActivity, previewSecond)
    const layout =
      widget.data.display_type === 'corner'
        ? getCornerGaugeLayout(widget.data, presentation.rawValue, presentation.values)
        : getArcGaugeLayout(widget.data, presentation.rawValue, presentation.values)
    const trackGeometry = {
      centerX: layout.centerX,
      centerY: layout.centerY,
      radius: layout.radius,
      startAngle: layout.startAngle,
      sweepAngle: layout.sweepAngle,
      trackThickness: layout.trackThickness,
    }
    const innerModel = buildArcGaugeInnerWidgetModel({ widget, presentationValue: presentation.value })
    const opacity = widget.data.opacity * globalOpacity
    const segmented = widget.data.track_fill_style === 'bars'
    const fillEndCornerRadius = widget.data.track_fill_flat ? 0 : widget.data.track_corner_radius
    const fillReveal = segmented
      ? null
      : getArcFilledTrackRevealSpec({
          ...trackGeometry,
          startCornerRadius: widget.data.track_corner_radius,
          endCornerRadius: fillEndCornerRadius,
          fill: layout.fill,
        })
    const barLayout = segmented
      ? getArcBarSegments({
          ...trackGeometry,
          borderThickness: widget.data.track_border_thickness,
          cornerRadius: widget.data.track_corner_radius,
          bar_count: widget.data.bar_count,
          bar_gap: widget.data.bar_gap,
        })
      : null
    const metricType = widget.data.value
    const displayUnit = widget.data.display_unit
    const minLabel = formatGaugeBoundaryLabel(metricType, layout.min + presentation.valueOffset, displayUnit)
    const maxLabel = formatGaugeBoundaryLabel(metricType, layout.max + presentation.valueOffset, displayUnit)
    const shadow = getTextShadowParts(sceneStyle)

    return {
      trackGeometry,
      innerModel,
      innerLayout: getArcInnerWidgetLayout(widget.data, layout, innerModel),
      opacity,
      fillReveal,
      barLayout,
      filledBarCount: barLayout ? getBarFillCount(layout.fill, barLayout.count) : 0,
      minLabel,
      maxLabel,
      labels: widget.data.show_min_max_labels
        ? getLabelLayout(layout, minLabel, maxLabel, labelFontFamily, widget.data.min_max_label_font_size)
        : null,
      labelFontFamily,
      shadow,
      trackShadow: widget.data.track_border_thickness > 0 ? shadow : undefined,
      maskPadding: layout.outerStrokeWidth + 1,
    }
  }, [activity, globalOpacity, labelFontFamily, previewSecond, sceneStyle, widget])
}
