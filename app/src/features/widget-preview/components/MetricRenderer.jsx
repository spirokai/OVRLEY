/**
 * Renders the overlay metric/gradient widget SVG preview — value text,
 * optional unit text, optional icon, and gradient triangle indicator.
 *
 * Handles two layout modes:
 * 1. Standard metric (speed, heartrate, cadence, power, time, temperature)
 *    with icon + value + units.
 * 2. Gradient with value text + triangle indicator (up/down/zero).
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object} props.activity - Activity data with series values.
 * @param {number} props.previewSecond - Current preview time in seconds.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {number} props.globalScale - Global scale multiplier.
 * @param {object|null} props.metricPreviewModel - Precomputed preview model (optional).
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @returns {JSX.Element|null} SVG or div element with metric widget preview, or null.
 */

import { buildMetricWidgetPreviewModel } from '../utils/metricWidgetPreviewModel'
import { METRIC_ICON_SVGS, DEFAULT_GRADIENT_TRIANGLE_WIDTH, GRADIENT_ZERO_LINE_WIDTH_PX } from '@/features/overlay-editor'
import { getPreviewFontFamily, getWidgetOpacity, measurePreviewText } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'
import { buildGradientTrianglePath, formatGradientValue, getGradientWidgetLayout } from '../utils/formatUtils'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { PreviewMetricIcon, PreviewSvgText } from './previewSvgComponents'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'

function splitGradientUnitSuffix(text) {
  return text.endsWith('%') ? [text.slice(0, -1), '%'] : [text, '']
}

export function OverlayMetricWidget({ widget, activity, previewSecond, globalOpacity, globalScale, metricPreviewModel, sceneStyle }) {
  // Base styling — resolve font, color, opacity, and shadow from widget config and scene
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const unitColor = widget.data.unit_color || '#ffffff'
  const widgetOpacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(sceneStyle)

  // Value text — use the precomputed preview model or fall back to defaults; gradient overrides the value text inline
  let valueText = metricPreviewModel?.valueText ?? '--'
  let unitText = metricPreviewModel?.unitText ?? ''

  if (widget.type === 'gradient') {
    valueText = `${formatGradientValue(widget, getInterpolatedActivityValue(activity, 'gradient', previewSecond))}%`
  }

  // Layout — compute the metric layout (icon + value + units) or gradient layout (value + triangle) depending on widget type
  const currentGradientValue = Number(getInterpolatedActivityValue(activity, 'gradient', previewSecond) ?? 0)
  const metricLayout =
    widget.type === 'gradient'
      ? null
      : ((
          metricPreviewModel ??
          buildMetricWidgetPreviewModel({
            widget,
            activity,
            previewSecond,
          })
        )?.metricLayout ?? null)
  const gradientLayout =
    widget.type === 'gradient'
      ? getGradientWidgetLayout({
          fontSize,
          fontFamily,
          valueText,
          valueOffset: widget.data.value_offset ?? 0,
          gradientValue: currentGradientValue,
          triangleWidth: widget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
          showTriangle: widget.data.show_triangle !== false,
          scale: globalScale || 1,
        })
      : null

  // Standard metric rendering — icon + value text + optional units text in an SVG overlay
  if (widget.type !== 'gradient' && metricLayout) {
    const previewModel =
      metricPreviewModel ??
      buildMetricWidgetPreviewModel({
        widget,
        activity,
        previewSecond,
      })
    const iconSvg = METRIC_ICON_SVGS[widget.type]
    const valueShadowFilterId = sanitizeSvgId(`${widget.id}-value-shadow`)
    const unitsShadowFilterId = sanitizeSvgId(`${widget.id}-units-shadow`)
    const iconShadowFilterId = sanitizeSvgId(`${widget.id}-icon-shadow`)
    const visualBounds = previewModel?.visualBounds
    const iconLeft = metricLayout.icon ? metricLayout.icon.left + (widget.data.icon_offset_x ?? 0) + (visualBounds?.offsetX ?? 0) : 0
    const iconTop = metricLayout.icon ? metricLayout.icon.top + (widget.data.icon_offset_y ?? 0) + (visualBounds?.offsetY ?? 0) : 0
    const renderWidth = visualBounds?.width ?? metricLayout.width
    const renderHeight = visualBounds?.height ?? metricLayout.height
    const contentOffsetX = visualBounds?.offsetX ?? 0
    const contentOffsetY = visualBounds?.offsetY ?? 0

    return (
      <div
        className="relative"
        style={{
          width: renderWidth,
          height: renderHeight,
        }}
      >
        <div className="absolute" style={{ width: renderWidth, height: renderHeight }}>
          <svg
            width={renderWidth}
            height={renderHeight}
            viewBox={`0 0 ${renderWidth} ${renderHeight}`}
            className="absolute left-0 top-0 block overflow-visible"
          >
            {metricLayout.icon && iconSvg ? (
              <PreviewMetricIcon
                icon={iconSvg}
                left={iconLeft}
                top={iconTop}
                size={metricLayout.icon.size}
                color={widget.data.icon_color || '#40e0d0'}
                opacity={widgetOpacity}
                shadow={shadow}
                shadowFilterId={shadow ? iconShadowFilterId : undefined}
              />
            ) : null}
            <PreviewSvgText
              text={valueText}
              x={metricLayout.value.left + contentOffsetX}
              baseline={metricLayout.value.baseline + contentOffsetY}
              color={color}
              fontFamily={fontFamily}
              fontSize={fontSize}
              opacity={widgetOpacity}
              shadow={shadow}
              shadowFilterId={valueShadowFilterId}
              borderColor={sceneStyle?.border_color}
              borderThickness={sceneStyle?.border_thickness}
            />
            {metricLayout.units ? (
              <PreviewSvgText
                text={unitText}
                x={metricLayout.units.left + contentOffsetX}
                baseline={metricLayout.units.baseline + contentOffsetY}
                color={unitColor}
                fontFamily={fontFamily}
                fontSize={metricLayout.units.fontSize}
                opacity={widgetOpacity}
                shadow={shadow}
                shadowFilterId={unitsShadowFilterId}
                borderColor={sceneStyle?.border_color}
                borderThickness={sceneStyle?.border_thickness}
              />
            ) : null}
          </svg>
        </div>
      </div>
    )
  }

  // Gradient rendering — value text + direction triangle (up for positive, down for negative, line for zero)
  if (widget.type === 'gradient' && gradientLayout) {
    const valueShadowFilterId = sanitizeSvgId(`${widget.id}-value-shadow`)
    const unitShadowFilterId = sanitizeSvgId(`${widget.id}-unit-shadow`)
    const trianglePath = gradientLayout.triangle
      ? buildGradientTrianglePath(currentGradientValue, gradientLayout.triangle.width, gradientLayout.triangle.height)
      : ''
    const [gradientValuePrefix, gradientUnitSuffix] = splitGradientUnitSuffix(valueText)
    const gradientPrefixWidth = gradientValuePrefix ? measurePreviewText(gradientValuePrefix, fontSize, fontFamily).width : 0

    return (
      <svg
        width={gradientLayout.width}
        height={gradientLayout.height}
        viewBox={`0 0 ${gradientLayout.width} ${gradientLayout.height}`}
        className="block overflow-visible"
      >
        {gradientValuePrefix ? (
          <PreviewSvgText
            text={gradientValuePrefix}
            x={gradientLayout.value.left}
            baseline={gradientLayout.value.baseline}
            color={color}
            fontFamily={fontFamily}
            fontSize={fontSize}
            opacity={widgetOpacity}
            shadow={shadow}
            shadowFilterId={valueShadowFilterId}
            borderColor={sceneStyle?.border_color}
            borderThickness={sceneStyle?.border_thickness}
          />
        ) : null}
        {gradientUnitSuffix ? (
          <PreviewSvgText
            text={gradientUnitSuffix}
            x={gradientLayout.value.left + gradientPrefixWidth}
            baseline={gradientLayout.value.baseline}
            color={unitColor}
            fontFamily={fontFamily}
            fontSize={fontSize}
            opacity={widgetOpacity}
            shadow={shadow}
            shadowFilterId={unitShadowFilterId}
            borderColor={sceneStyle?.border_color}
            borderThickness={sceneStyle?.border_thickness}
          />
        ) : null}
        {gradientLayout.triangle ? (
          gradientLayout.triangle.isZero ? (
            <line
              x1={gradientLayout.triangle.left}
              y1={gradientLayout.triangle.baseline}
              x2={gradientLayout.triangle.left + gradientLayout.triangle.width}
              y2={gradientLayout.triangle.baseline}
              stroke={widget.data.triangle_positive_color || '#40e0d0'}
              strokeWidth={GRADIENT_ZERO_LINE_WIDTH_PX}
              opacity={widgetOpacity}
              strokeLinecap="round"
            />
          ) : trianglePath ? (
            <path
              d={trianglePath}
              transform={`translate(${gradientLayout.triangle.left} ${gradientLayout.triangle.baseline})`}
              fill={currentGradientValue < 0 ? widget.data.triangle_negative_color || '#c65102' : widget.data.triangle_positive_color || '#40e0d0'}
              opacity={widgetOpacity}
            />
          ) : null
        ) : null}
      </svg>
    )
  }

  return null
}
