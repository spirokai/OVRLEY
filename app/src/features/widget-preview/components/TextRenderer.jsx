/**
 * Renders the overlay text/label widget SVG preview — renders widget text
 * with font, color, opacity, shadow, and border styling.
 *
 * All data is received via props; no store access.
 */

import { buildTextWidgetPreviewModel } from '../utils/textWidgetPreviewModel'
import { getPreviewFontFamily } from '../utils/textMeasurement'
import { getWidgetOpacity } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { PreviewSvgText } from './previewSvgComponents'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'

export function OverlayTextWidget({ widget, globalOpacity, sceneStyle, textPreviewModel }) {
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const opacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(sceneStyle)
  const previewModel = textPreviewModel ?? buildTextWidgetPreviewModel({ widget })
  const visualBounds = previewModel?.visualBounds
  const renderWidth = visualBounds?.width ?? previewModel?.measurement?.width ?? 0
  const renderHeight = visualBounds?.height ?? previewModel?.lineHeight ?? 0

  return (
    <svg width={renderWidth} height={renderHeight} viewBox={`0 0 ${renderWidth} ${renderHeight}`} className="block overflow-visible">
      <PreviewSvgText
        text={previewModel?.text ?? 'TEXT'}
        x={visualBounds?.offsetX ?? 0}
        baseline={(previewModel?.baseline ?? 0) + (visualBounds?.offsetY ?? 0)}
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
