import { useMemo } from 'react'
import { getInterpolatedActivityValue } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { GRADIENT_ZERO_LINE_WIDTH_PX } from '@/features/overlay-editor/data/overlayEditorConstants'
import { METRIC_ICON_SVGS } from '@/lib/widget/widget-icon-data'
import { buildGradientTrianglePath, formatGradientValue, getGradientWidgetLayout } from './format'
import { buildMetricWidgetPreviewModel } from './model'
import { getPreviewFontFamily, getWidgetOpacity, measurePreviewText } from '../../shared/textMeasurement'
import { getTextShadowParts } from '../../shared/shadow'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'

function splitGradientUnitSuffix(text) {
  return text.endsWith('%') ? [text.slice(0, -1), '%'] : [text, '']
}

function buildMetricTextRuns({ widget, content, visualBounds, shadowFilterIds }) {
  if (content.type === 'coordinates') {
    const runs = []
    for (let index = 0; index < content.layout.lines.length; index += 1) {
      const line = content.layout.lines[index]
      if (line.direction) {
        runs.push({
          key: `coordinate-direction-${index}`,
          text: line.direction,
          x: line.directionLeft + visualBounds.offsetX,
          baseline: line.baseline + visualBounds.offsetY,
          color: line.directionColor,
          fontSize: content.layout.fontSize,
          shadowFilterId: shadowFilterIds.value,
        })
      }
      runs.push({
        key: `coordinate-value-${index}`,
        text: line.valueText,
        x: line.valueLeft + visualBounds.offsetX,
        baseline: line.baseline + visualBounds.offsetY,
        color: widget.data.color,
        fontSize: content.layout.fontSize,
        shadowFilterId: shadowFilterIds.value,
      })
    }
    return runs
  }

  const { layout, valueText, unitText } = content
  return [
    {
      key: 'value',
      text: valueText,
      x: layout.value.left + visualBounds.offsetX,
      baseline: layout.value.baseline + visualBounds.offsetY,
      color: widget.data.color,
      fontSize: widget.data.font_size,
      shadowFilterId: shadowFilterIds.value,
    },
    ...(layout.units
      ? [
          {
            key: 'units',
            text: unitText,
            x: layout.units.left + visualBounds.offsetX,
            baseline: layout.units.baseline + visualBounds.offsetY,
            color: widget.data.unit_color,
            fontSize: layout.units.fontSize,
            shadowFilterId: shadowFilterIds.units,
          },
        ]
      : []),
  ]
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
  const fontFamily = getPreviewFontFamily(widget.data.font)
  useFontMetrics([{ fontFamily, fontSize: widget.data.font_size }])

  return useMemo(() => {
    // Shared presentation: these values apply to both metric and gradient modes.
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
          globalScale,
        }))

    let valueText
    const currentGradientValue = getInterpolatedActivityValue(activity, 'gradient', previewSecond) ?? 0

    // Gradient values are formatted inline because they depend on the live activity sample.
    if (isGradient) {
      valueText = `${formatGradientValue(widget, getInterpolatedActivityValue(activity, 'gradient', previewSecond))}%`
    }

    // Layout selection: only gradient widgets need triangle/value layout.
    const gradientLayout = isGradient
      ? getGradientWidgetLayout({
          fontSize: widget.data.font_size,
          fontFamily,
          valueText,
          valueOffset: widget.data.value_offset,
          gradientValue: currentGradientValue,
          triangleWidth: widget.data.triangle_width,
          showTriangle: widget.data.show_triangle,
          scale: globalScale,
        })
      : null

    if (!isGradient) {
      // Standard metric mode: icon/value/unit layout comes from the preview model.
      const { content } = previewModel
      const metricLayout = content.layout
      const visualBounds = previewModel.visualBounds
      const shadowFilterIds = {
        value: sanitizeSvgId(`${widget.id}-value-shadow`),
        units: sanitizeSvgId(`${widget.id}-units-shadow`),
      }

      return {
        mode: 'metric',
        fontFamily,
        widgetOpacity,
        shadow,
        iconSvg: METRIC_ICON_SVGS[widget.type],
        metricLayout,
        textRuns: buildMetricTextRuns({
          widget,
          content,
          visualBounds,
          shadowFilterIds,
        }),
        visualBounds,
        iconLeft: metricLayout.icon ? metricLayout.icon.left + widget.data.icon_offset_x + visualBounds.offsetX : 0,
        iconTop: metricLayout.icon ? metricLayout.icon.top + widget.data.icon_offset_y + visualBounds.offsetY : 0,
        valueShadowFilterId: shadowFilterIds.value,
        unitsShadowFilterId: shadowFilterIds.units,
        iconShadowFilterId: sanitizeSvgId(`${widget.id}-icon-shadow`),
      }
    }

    // Gradient mode: split the rendered text into numeric prefix and percent suffix.
    const trianglePath = gradientLayout.triangle
      ? buildGradientTrianglePath(currentGradientValue, gradientLayout.triangle.width, gradientLayout.triangle.height)
      : ''
    const [gradientValuePrefix, gradientUnitSuffix] = splitGradientUnitSuffix(valueText)
    const gradientPrefixWidth = gradientValuePrefix ? measurePreviewText(gradientValuePrefix, widget.data.font_size, fontFamily).width : 0

    return {
      mode: 'gradient',
      fontFamily,
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
  }, [activity, fontFamily, globalOpacity, globalScale, metricPreviewModel, previewSecond, sceneStyle, widget])
}
