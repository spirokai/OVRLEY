import { useMemo } from 'react'
import { getInterpolatedActivityValue, GRADIENT_ZERO_LINE_WIDTH_PX, METRIC_ICON_SVGS } from '@/features/overlay-editor'
import { buildGradientTrianglePath, formatGradientValue, getGradientWidgetLayout } from '../utils/formatUtils'
import { buildMetricWidgetPreviewModel } from '../utils/metricWidgetPreviewUtils'
import { getPreviewFontFamily, getWidgetOpacity, measurePreviewText } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from './useFontMetricsVersion'

function splitGradientUnitSuffix(text) {
  return text.endsWith('%') ? [text.slice(0, -1), '%'] : [text, '']
}

/**
 * Builds the presentation model for the metric preview renderer.
 *
 * Centralizes all non-JSX preparation for both standard metric and gradient
 * widgets: font/shadow setup, preview-model resolution, layout selection,
 * icon positioning, gradient value formatting, and derived SVG ids.
 *
 * Stages:
 * 1. Resolve shared font, color, opacity, and shadow state.
 * 2. Branch into standard-metric or gradient presentation mode.
 * 3. Build the mode-specific layout/presentation model consumed by the renderer.
 *
 * @param {object} params - Metric preview inputs.
 * @param {object} params.widget - Effective metric widget.
 * @param {object} params.activity - Activity data with metric series.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {number} params.globalOpacity - Global opacity multiplier.
 * @param {number} params.globalScale - Scene/global scale applied to the preview.
 * @param {object|null} params.metricPreviewModel - Optional precomputed metric preview model.
 * @param {object} params.sceneStyle - Scene style object.
 * @returns {object} Presentation model consumed by the metric preview renderer.
 */
export function useMetricPreviewPresentation({ widget, activity, previewSecond, globalOpacity, globalScale, metricPreviewModel, sceneStyle }) {
  // Typography: ensure font metrics are loaded before layout-dependent rendering.
  const fontSize = widget.data.font_size
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  useFontMetricsVersion(fontFamily, fontSize)

  return useMemo(() => {
    // Shared presentation: these values apply to both metric and gradient modes.
    const color = widget.data.color
    const unitColor = widget.data.unit_color
    const widgetOpacity = getWidgetOpacity(widget.data, globalOpacity)
    const shadow = getTextShadowParts(sceneStyle)
    const isGradient = widget.type === 'gradient'
    const previewModel = isGradient
      ? null
      : (metricPreviewModel ??
        buildMetricWidgetPreviewModel({
          widget,
          activity,
          previewSecond,
        }))

    let valueText = metricPreviewModel?.valueText
    let unitText = metricPreviewModel?.unitText
    const currentGradientValue = Number(getInterpolatedActivityValue(activity, 'gradient', previewSecond) ?? 0)

    // Gradient values are formatted inline because they depend on the live activity sample.
    if (isGradient) {
      valueText = `${formatGradientValue(widget, getInterpolatedActivityValue(activity, 'gradient', previewSecond))}%`
      unitText = ''
    }

    // Layout selection: only gradient widgets need triangle/value layout.
    const gradientLayout = isGradient
      ? getGradientWidgetLayout({
          fontSize,
          fontFamily,
          valueText,
          valueOffset: widget.data.value_offset,
          gradientValue: currentGradientValue,
          triangleWidth: widget.data.triangle_width,
          showTriangle: widget.data.show_triangle,
          scale: globalScale ?? 1,
        })
      : null

    if (!isGradient) {
      // Standard metric mode: icon/value/unit layout comes from the preview model.
      const metricLayout = previewModel.metricLayout
      const visualBounds = previewModel?.visualBounds
      const contentOffsetX = visualBounds?.offsetX ?? 0
      const contentOffsetY = visualBounds?.offsetY ?? 0

      valueText = previewModel.valueText
      unitText = previewModel.unitText

      return {
        mode: 'metric',
        fontSize,
        fontFamily,
        color,
        unitColor,
        widgetOpacity,
        shadow,
        valueText,
        unitText,
        iconSvg: METRIC_ICON_SVGS[widget.type],
        metricLayout,
        renderWidth: visualBounds?.width ?? metricLayout?.width ?? 0,
        renderHeight: visualBounds?.height ?? metricLayout?.height ?? 0,
        contentOffsetX,
        contentOffsetY,
        iconLeft: metricLayout?.icon ? metricLayout.icon.left + (widget.data.icon_offset_x ?? 0) + contentOffsetX : 0,
        iconTop: metricLayout?.icon ? metricLayout.icon.top + (widget.data.icon_offset_y ?? 0) + contentOffsetY : 0,
        valueShadowFilterId: sanitizeSvgId(`${widget.id}-value-shadow`),
        unitsShadowFilterId: sanitizeSvgId(`${widget.id}-units-shadow`),
        iconShadowFilterId: sanitizeSvgId(`${widget.id}-icon-shadow`),
      }
    }

    // Gradient mode: split the rendered text into numeric prefix and percent suffix.
    const trianglePath = gradientLayout?.triangle
      ? buildGradientTrianglePath(currentGradientValue, gradientLayout.triangle.width, gradientLayout.triangle.height)
      : ''
    const [gradientValuePrefix, gradientUnitSuffix] = splitGradientUnitSuffix(valueText)
    const gradientPrefixWidth = gradientValuePrefix ? measurePreviewText(gradientValuePrefix, fontSize, fontFamily).width : 0

    return {
      mode: 'gradient',
      fontSize,
      fontFamily,
      color,
      unitColor,
      widgetOpacity,
      shadow,
      currentGradientValue,
      gradientLayout,
      gradientValuePrefix,
      gradientUnitSuffix,
      gradientPrefixWidth,
      trianglePath,
      valueShadowFilterId: sanitizeSvgId(`${widget.id}-value-shadow`),
      unitShadowFilterId: sanitizeSvgId(`${widget.id}-unit-shadow`),
      gradientZeroLineWidth: GRADIENT_ZERO_LINE_WIDTH_PX,
      positiveTriangleColor: widget.data.triangle_positive_color,
      negativeTriangleColor: widget.data.triangle_negative_color,
    }
  }, [activity, fontFamily, fontSize, globalOpacity, globalScale, metricPreviewModel, previewSecond, sceneStyle, widget])
}
