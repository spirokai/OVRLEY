import { useMemo } from 'react'
import { getPreviewActivity } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { getTextShadowParts } from '../../shared/shadow'
import {
  getPreviewFontFamily,
  getPreviewTextBaseline,
  getPreviewVerticalMetrics,
  getWidgetOpacity,
  measurePreviewText,
} from '../../shared/textMeasurement'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'
import { buildGForceFrameState, prepareGForcePreview } from './model'

const GUIDE_DEFAULT_THICKNESS_PX = 2
const GUIDE_REFERENCE_DIAMETER_PX = 400
const GUIDE_OPACITY = 0.4
const GUIDE_RING_RADIUS_RATIOS = [0.33, 0.66]

/** Builds the complete SVG presentation model for the current G-force frame. */
export function useGForcePreviewModel({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle }) {
  const fontFamily = getPreviewFontFamily(widget.data.label_font)
  const fontMetricsVersion = useFontMetrics([{ fontFamily, fontSize: widget.data.label_font_size }])
  const config = widget.data
  const { axis_horizontal, axis_vertical, clip_percentile, invert_horizontal, invert_vertical } = config
  const prepared = useMemo(
    () =>
      prepareGForcePreview(activity, {
        axis_horizontal,
        axis_vertical,
        clip_percentile,
        invert_horizontal,
        invert_vertical,
      }),
    [activity, axis_horizontal, axis_vertical, clip_percentile, invert_horizontal, invert_vertical],
  )

  return useMemo(() => {
    const centerX = config.width / 2
    const centerY = config.height / 2
    const radius = config.diameter / 2
    const displayActivity = getPreviewActivity(activity, previewSecond)
    const frame = buildGForceFrameState(prepared, config, previewSecond, centerX, centerY, radius, displayActivity)
    const lineHeight = config.label_font_size * 0.92
    const margin = config.label_font_size * 0.05
    const top = centerY + radius - lineHeight - margin + config.label_offset_y
    const valueWidth = measurePreviewText(frame.valueText, config.label_font_size, fontFamily).width
    const unitGap = frame.unitText ? 3 / globalScale : 0
    const verticalMetrics = getPreviewVerticalMetrics(frame.valueText, config.label_font_size, fontFamily)
    const coordinateMetrics = getPreviewVerticalMetrics(frame.coordinateText, config.label_font_size, fontFamily)
    const componentMetrics = getPreviewVerticalMetrics(frame.componentText, config.label_font_size, fontFamily)
    const valueX = centerX + radius + margin + config.label_offset_x

    return {
      ...frame,
      maxG: prepared.maxG,
      centerX,
      centerY,
      radius,
      borderRadius: radius - config.border_thickness / 2,
      innerRadius: radius - config.border_thickness,
      guideColor: config.marker_color,
      guideOpacity: GUIDE_OPACITY,
      guideThickness: GUIDE_DEFAULT_THICKNESS_PX * (config.diameter / GUIDE_REFERENCE_DIAMETER_PX),
      guideRadii: GUIDE_RING_RADIUS_RATIOS.map((ratio) => radius * ratio),
      markerRadius: config.marker_size / 2,
      opacity: getWidgetOpacity(config, globalOpacity),
      fontFamily,
      fontMetricsVersion,
      labelX: valueX,
      unitX: valueX + valueWidth + unitGap,
      labelBaseline: getPreviewTextBaseline({ top, lineHeight, ...verticalMetrics }),
      coordinateX: centerX - radius + margin,
      coordinateBaseline: getPreviewTextBaseline({ top: centerY - radius + margin, lineHeight, ...coordinateMetrics }),
      componentBaseline: getPreviewTextBaseline({ top: centerY - radius + margin + lineHeight, lineHeight, ...componentMetrics }),
      shadow: getTextShadowParts(sceneStyle),
      borderShadowFilterId: sanitizeSvgId(`${widget.id}-g-force-border-shadow`),
      labelShadowFilterId: sanitizeSvgId(`${widget.id}-g-force-label-shadow`),
      guideClipId: sanitizeSvgId(`${widget.id}-g-force-guide-clip`),
    }
  }, [activity, config, fontFamily, fontMetricsVersion, globalOpacity, globalScale, prepared, previewSecond, sceneStyle, widget.id])
}
