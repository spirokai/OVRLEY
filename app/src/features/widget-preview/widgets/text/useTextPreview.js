import { buildTextWidgetPreviewModel } from './model'
import { getPreviewFontFamily, getWidgetOpacity } from '../../shared/textMeasurement'
import { getTextShadowParts } from '../../shared/shadow'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'

/** @param {object} props Label preview inputs. @returns {object} SVG presentation. */
export function useTextPreview({ widget, globalOpacity, sceneStyle, textPreviewModel }) {
  const fontSize = widget.data.font_size
  const fontFamily = getPreviewFontFamily(widget.data.font)
  return {
    fontSize,
    fontFamily,
    opacity: getWidgetOpacity(widget.data, globalOpacity),
    shadow: getTextShadowParts(sceneStyle),
    shadowFilterId: sanitizeSvgId(`${widget.id}-label-shadow`),
    previewModel: textPreviewModel ?? buildTextWidgetPreviewModel({ widget }),
  }
}
