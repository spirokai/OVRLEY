import { useId } from 'react'
import { getPreviewActivity } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { getBarFillCount, getLinearBarRects } from '../../shared/gaugeBarGeometry'
import {
  getLinearGaugeLabelLayout,
  getLinearGaugeLayout,
  getLinearRectCornerRadii,
  getLinearSegmentModels,
  getLinearTranslatedFillPath,
} from './geometry'
import { getTextShadowParts } from '../../shared/shadow'
import { normalizeSvgShadowColor } from '../../shared/svgPreviewUtils'
import { getPreviewFontFamily } from '../../shared/textMeasurement'
import { useFontMetrics } from '../../shared/useFontMetrics'
import { formatGaugeBoundaryLabel } from '../../shared/gaugeLabelFormat'
import { resolveMetricPresentationValues } from '@/lib/widget/altitude-correction'

/** Builds all non-JSX state for a normalized linear-gauge preview. */
export function useLinearGaugePreviewPresentation({ widget, activity, previewSecond, globalOpacity, sceneStyle }) {
  const maskId = useId()
  const labelFontFamily = getPreviewFontFamily(widget.data.min_max_label_font)
  useFontMetrics([{ fontFamily: labelFontFamily, fontSize: widget.data.min_max_label_font_size }])

  const displayActivity = getPreviewActivity(activity, previewSecond)
  const presentation = resolveMetricPresentationValues(widget, displayActivity, previewSecond)
  const layout = getLinearGaugeLayout({
    value: presentation.rawValue,
    values: presentation.values,
    width: widget.data.width,
    height: widget.data.height,
    orientation: widget.data.orientation,
    borderThickness: widget.data.track_border_thickness,
  })
  const segmented = widget.data.track_fill_style === 'bars'
  const bars = segmented
    ? getLinearBarRects(widget.data)
    : { count: 1, rects: [{ x: 0, y: 0, width: widget.data.width, height: widget.data.height }] }
  const segments = getLinearSegmentModels(bars.rects, widget.data.track_border_thickness, widget.data.track_corner_radius)
  const opacity = widget.data.opacity * (globalOpacity ?? 1)
  const trackShadow = widget.data.track_border_thickness > 0 ? getTextShadowParts(sceneStyle) : undefined
  const fillCornerRadius = Math.max(0, widget.data.track_corner_radius - widget.data.track_border_thickness)
  const innerTrackCornerRadii = getLinearRectCornerRadii(fillCornerRadius, layout.innerTrackRect)
  const translatedFillPath =
    fillCornerRadius > 0 && !widget.data.track_fill_flat
      ? getLinearTranslatedFillPath({
          trackRect: layout.innerTrackRect,
          fillRect: layout.fillRect,
          orientation: widget.data.orientation,
          cornerRadius: fillCornerRadius,
        })
      : ''
  const metricType = widget.data.value
  const displayUnit = widget.data.display_unit
  const minLabel = formatGaugeBoundaryLabel(metricType, layout.min + presentation.valueOffset, displayUnit)
  const maxLabel = formatGaugeBoundaryLabel(metricType, layout.max + presentation.valueOffset, displayUnit)

  return {
    maskId,
    flatFillClipId: `${maskId}-flat-fill`,
    innerTrackClipId: `${maskId}-inner-track`,
    layout,
    segmented,
    segments,
    filledCount: segmented ? getBarFillCount(layout.fill, bars.count) : 0,
    opacity,
    trackShadow,
    shadowColor: trackShadow ? normalizeSvgShadowColor(trackShadow.color, opacity) : null,
    shadowFilterId: trackShadow ? `linear-gauge-${widget.id}-shadow` : null,
    fillCornerRadius,
    innerTrackCornerRadii,
    translatedFillPath,
    minLabel,
    maxLabel,
    labelFontFamily,
    labelLayout: widget.data.show_min_max_labels ? getLinearGaugeLabelLayout({ data: widget.data, labelFontFamily, minLabel, maxLabel }) : null,
  }
}
