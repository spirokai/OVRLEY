import { getPreviewFontFamily, getPreviewTextBaseline, getPreviewVerticalMetrics, measurePreviewText } from '../../shared/textMeasurement'
import { getPreviewActivity } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { getLapTimerDisplayValue } from './lapTimer'

/**
 * Builds the intrinsic text layout for a Current Lap or Best Lap widget.
 * @param {object} params
 * @param {object} params.widget - Lap timer widget configuration.
 * @param {object|null} params.activity - Parsed activity with lap timing data.
 * @param {number} params.previewSecond - Absolute activity timestamp.
 * @returns {object} Lap timer preview model.
 */
export function buildLapTimerPreviewModel({ widget, activity, previewSecond }) {
  const fontFamily = getPreviewFontFamily(widget.data.font)
  const displayActivity = getPreviewActivity(activity, previewSecond)
  const valueText = getLapTimerDisplayValue(displayActivity, previewSecond, widget.data.lap_timer_mode)
  const valueLineHeight = widget.data.font_size * 0.92
  const valueMeasure = measurePreviewText(valueText, widget.data.font_size, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, widget.data.font_size, fontFamily)
  const showLabel = widget.data.show_label
  const labelFontSize = widget.data.font_size * 0.35
  const labelLineHeight = labelFontSize * 0.92
  const labelMeasure = showLabel ? measurePreviewText(widget.data.label, labelFontSize, fontFamily) : null
  const labelVerticalMetrics = showLabel ? getPreviewVerticalMetrics(widget.data.label, labelFontSize, fontFamily) : null
  const labelBaseline = showLabel
    ? getPreviewTextBaseline({
        lineHeight: labelLineHeight,
        ascent: labelVerticalMetrics.ascent,
        glyphHeight: labelVerticalMetrics.glyphHeight,
      })
    : null
  const valueTop = showLabel ? labelLineHeight : 0
  const valueBaseline = getPreviewTextBaseline({
    top: valueTop,
    lineHeight: valueLineHeight,
    ascent: valueVerticalMetrics.ascent,
    glyphHeight: valueVerticalMetrics.glyphHeight,
  })

  const segments = [
    ...(showLabel ? [{ left: 0, baseline: labelBaseline, measure: labelMeasure }] : []),
    { left: 0, baseline: valueBaseline, measure: valueMeasure },
  ]
  const minX = Math.min(...segments.map(({ left, measure }) => left - measure.boundsLeft))
  const minY = Math.min(...segments.map(({ baseline, measure }) => baseline - measure.ascent))
  const maxX = Math.max(...segments.map(({ left, measure }) => left + measure.boundsRight))
  const maxY = Math.max(...segments.map(({ baseline, measure }) => baseline + measure.descent))

  return {
    content: {
      type: 'lap_timer',
      labelText: showLabel ? widget.data.label : '',
      valueText,
      labelFontSize,
      labelBaseline,
      valueBaseline,
    },
    valueText,
    visualBounds: {
      minX,
      minY,
      maxX,
      maxY,
      width: Math.max(maxX - minX, 0),
      height: Math.max(maxY - minY, 0),
      offsetX: -minX,
      offsetY: -minY,
    },
  }
}
