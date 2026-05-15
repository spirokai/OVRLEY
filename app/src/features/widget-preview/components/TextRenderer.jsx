/**
 * Renders the overlay text/label widget SVG preview — renders widget text
 * with font, color, opacity, shadow, and border styling.
 *
 * All data is received via props; no store access.
 */

import { getPreviewFontFamily, getPreviewTextBaseline, measurePreviewText } from '../utils/textMeasurement'
import { getWidgetOpacity } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { METRIC_WIDGET_LINE_HEIGHT } from '@/features/overlay-editor'
import { PreviewSvgText } from './previewSvgComponents'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'

export function OverlayTextWidget({ widget, globalOpacity, sceneStyle }) {
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const opacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(sceneStyle)
  const text = widget.data.text || 'TEXT'
  const lineHeight = fontSize * METRIC_WIDGET_LINE_HEIGHT
  const measurement = measurePreviewText(text, fontSize, fontFamily)
  const baseline = getPreviewTextBaseline({
    top: 0,
    lineHeight,
    ascent: measurement.ascent,
    descent: measurement.descent,
    glyphHeight: measurement.glyphHeight,
  })

  return (
    <svg width={measurement.width} height={lineHeight} viewBox={`0 0 ${measurement.width} ${lineHeight}`} className="block overflow-visible">
      <PreviewSvgText
        text={text}
        baseline={baseline}
        color={color}
        fontFamily={fontFamily}
        fontSize={fontSize}
        opacity={opacity}
        shadow={shadow}
        shadowFilterId={sanitizeSvgId(`${widget.id}-label-shadow`)}
        borderColor={sceneStyle?.border_color}
        borderThickness={sceneStyle?.border_thickness}
      />
    </svg>
  )
}
