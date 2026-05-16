/**
 * Builds the shared preview model for text/label widgets.
 */

import { METRIC_WIDGET_LINE_HEIGHT } from '@/features/overlay-editor'
import { getPreviewFontFamily, getPreviewTextBaseline, measurePreviewText } from './textMeasurement'

export function buildTextWidgetPreviewModel({ widget }) {
  if (!widget || widget.type !== 'label') {
    return null
  }

  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  const text = (widget.data.text || 'TEXT').toUpperCase()
  const lineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const measurement = measurePreviewText(text, fontSize, fontFamily)
  const baseline = getPreviewTextBaseline({
    top: 0,
    lineHeight,
    ascent: measurement.ascent,
    glyphHeight: measurement.glyphHeight,
  })

  const minX = -(measurement.boundsLeft ?? 0)
  const minY = baseline - (measurement.ascent ?? 0)
  const maxX = measurement.boundsRight ?? measurement.width ?? 0
  const maxY = baseline + (measurement.descent ?? 0)
  const width = Math.max(maxX - minX, 0)
  const height = Math.max(maxY - minY, 0)

  return {
    baseline,
    fontFamily,
    fontSize,
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
