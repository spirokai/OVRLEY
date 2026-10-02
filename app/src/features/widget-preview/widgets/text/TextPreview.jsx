/**
 * Renders the overlay text/label widget SVG preview — renders widget text
 * with font, color, opacity, shadow, and border styling.
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {object|null} props.textPreviewModel - Precomputed text preview model (optional).
 * @returns {JSX.Element} SVG element for text widget preview.
 */

import { useTextPreview } from './useTextPreview'
import { PreviewSvgText } from '../../shared/PreviewSvgComponents'

export function OverlayTextWidget({ widget, globalOpacity, sceneStyle, textPreviewModel }) {
  const { fontSize, fontFamily, opacity, shadow, shadowFilterId, previewModel } = useTextPreview({
    widget,
    globalOpacity,
    sceneStyle,
    textPreviewModel,
  })
  const visualBounds = previewModel.visualBounds

  return (
    <svg
      width={visualBounds.width}
      height={visualBounds.height}
      viewBox={`0 0 ${visualBounds.width} ${visualBounds.height}`}
      className="block overflow-visible"
    >
      <PreviewSvgText
        text={previewModel.text}
        textRuns={previewModel.measurement.runs}
        preserveWhitespace
        x={visualBounds.offsetX}
        baseline={previewModel.baseline + visualBounds.offsetY}
        color={widget.data.color}
        fontFamily={fontFamily}
        fontSize={fontSize}
        fontWeight={previewModel.fontWeight}
        fontStyle={previewModel.fontStyle}
        opacity={opacity}
        shadow={shadow}
        shadowFilterId={shadowFilterId}
        borderColor={sceneStyle?.border_color}
        borderThickness={sceneStyle?.border_thickness}
        textTransform="none"
      />
    </svg>
  )
}
