/**
 * Builds the shared preview model for text/label widgets.
 *
 * Computes text measurement, baseline, line height, font family, and visual
 * bounding box for a text/label widget.
 *
 * @param {object} params
 * @param {object} params.widget - Widget configuration object (must have type 'label').
 * @returns {object} Preview model with measurement, baseline, visualBounds, and text properties.
 */

import { METRIC_WIDGET_LINE_HEIGHT } from '@/features/overlay-editor/data/overlayEditorConstants'
import { resolveFontStyle } from '@/lib/fonts'
import { getPreparedFont } from '@/lib/font-resources'
import { getPreviewFontFamily, getPreviewTextBaseline, measurePreviewText } from '../../shared/textMeasurement'

export function buildTextWidgetPreviewModel({ widget }) {
  const fontSize = widget.data.font_size
  const fontFamily = getPreviewFontFamily(widget.data.font)
  const text = widget.data.text
  const lineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const { weight: fontWeight, fontStyle } = resolveFontStyle(getPreparedFont(widget.data.font), widget.data.font_weight, widget.data.italic)
  const letterSpacing = (fontSize * widget.data.letter_spacing) / 100
  const measurement = measurePreviewText(text, fontSize, fontFamily, fontWeight, fontStyle, letterSpacing)
  const baseline = getPreviewTextBaseline({
    top: 0,
    lineHeight,
    ascent: measurement.ascent,
    glyphHeight: measurement.glyphHeight,
  })

  const minX = -measurement.boundsLeft
  const minY = baseline - measurement.ascent
  const maxX = measurement.boundsRight
  const maxY = baseline + measurement.descent
  const width = Math.max(maxX - minX, 0)
  const height = Math.max(maxY - minY, 0)

  return {
    baseline,
    fontFamily,
    fontSize,
    fontWeight,
    fontStyle,
    lineHeight,
    measurement,
    text,
    visualBounds: {
      minX,
      minY,
      maxX,
      maxY,
      width,
      height,
      offsetX: -minX,
      offsetY: -minY,
    },
  }
}
