import { buildLapTimerPreviewModel } from './model'
import { getPreviewFontFamily, getWidgetOpacity } from '../../shared/textMeasurement'
import { getTextShadowParts } from '../../shared/shadow'
import { PreviewSvgText } from '../../shared/PreviewSvgComponents'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'

/**
 * Renders a lap timer text widget.
 * @param {object} props - Widget preview props.
 * @returns {JSX.Element} Lap timer SVG preview.
 */
export function OverlayLapTimerWidget({ widget, activity, previewSecond, globalOpacity, metricPreviewModel, sceneStyle }) {
  const fontFamily = getPreviewFontFamily(widget.data.font)
  useFontMetrics([{ fontFamily, fontSize: widget.data.font_size }])
  const opacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(sceneStyle)
  const model = metricPreviewModel ?? buildLapTimerPreviewModel({ widget, activity, previewSecond })
  const { content, visualBounds } = model

  return (
    <svg
      width={visualBounds.width}
      height={visualBounds.height}
      viewBox={`0 0 ${visualBounds.width} ${visualBounds.height}`}
      className="block overflow-visible"
    >
      {content.labelText ? (
        <PreviewSvgText
          text={content.labelText}
          x={visualBounds.offsetX}
          baseline={content.labelBaseline + visualBounds.offsetY}
          color={widget.data.color}
          fontFamily={fontFamily}
          fontSize={content.labelFontSize}
          opacity={opacity}
          shadow={shadow}
          shadowFilterId={sanitizeSvgId(`${widget.id}-lap-label-shadow`)}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
      ) : null}
      <PreviewSvgText
        text={content.valueText}
        x={visualBounds.offsetX}
        baseline={content.valueBaseline + visualBounds.offsetY}
        color={content.valueColor}
        fontFamily={fontFamily}
        fontSize={widget.data.font_size}
        opacity={opacity}
        shadow={shadow}
        shadowFilterId={sanitizeSvgId(`${widget.id}-lap-value-shadow`)}
        borderColor={sceneStyle?.border_color}
        borderThickness={sceneStyle?.border_thickness}
      />
    </svg>
  )
}
