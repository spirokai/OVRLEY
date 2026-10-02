import { buildTextWidgetPreviewModel } from './model'
import { getPreviewFontFamily, getWidgetOpacity } from '../../shared/textMeasurement'
import { getTextShadowParts } from '../../shared/shadow'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'

/** @param {object} props Label preview inputs. @returns {object} SVG presentation. */
export function useTextPreview({ widget, globalOpacity, sceneStyle, textPreviewModel }) {
  const fontSize = widget.data.font_size
  const fontFamily = getPreviewFontFamily(widget.data.font)
  useFontMetrics([{ fontId: widget.data.font, fontSize, fontWeight: widget.data.font_weight, italic: widget.data.italic }])
  return {
    fontSize,
    fontFamily,
    opacity: getWidgetOpacity(widget.data, globalOpacity),
    shadow: getTextShadowParts(sceneStyle),
    shadowFilterId: sanitizeSvgId(`${widget.id}-label-shadow`),
    previewModel: textPreviewModel ?? buildTextWidgetPreviewModel({ widget }),
  }
}
