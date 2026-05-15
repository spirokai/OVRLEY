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
 */

import { buildMetricWidgetPreviewModel } from '../utils/metricWidgetPreviewModel'
import { METRIC_ICON_SVGS, DEFAULT_GRADIENT_TRIANGLE_WIDTH, GRADIENT_ZERO_LINE_WIDTH_PX } from '@/features/overlay-editor'
import { getPreviewFontFamily, getWidgetOpacity } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'
import { buildGradientTrianglePath, formatGradientValue, getGradientWidgetLayout } from '../utils/formatUtils'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { PreviewMetricIcon, PreviewSvgText } from './previewSvgComponents'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'

export function OverlayMetricWidget({ widget, activity, previewSecond, globalOpacity, globalScale, metricPreviewModel, sceneStyle }) {
  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
  useFontMetricsVersion(fontFamily, fontSize)
  const color = widget.data.color || '#ffffff'
  const widgetOpacity = getWidgetOpacity(widget.data, globalOpacity)
  const shadow = getTextShadowParts(sceneStyle)

  let valueText = metricPreviewModel?.valueText ?? '--'
  let unitText = metricPreviewModel?.unitText ?? ''

  if (widget.type === 'gradient') {
    valueText = `${formatGradientValue(widget, getInterpolatedActivityValue(activity, 'gradient', previewSecond))}%`
  }

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
                color={color}
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

  if (widget.type === 'gradient' && gradientLayout) {
    const valueShadowFilterId = sanitizeSvgId(`${widget.id}-value-shadow`)
    const trianglePath = gradientLayout.triangle
      ? buildGradientTrianglePath(currentGradientValue, gradientLayout.triangle.width, gradientLayout.triangle.height)
      : ''

    return (
      <svg
        width={gradientLayout.width}
        height={gradientLayout.height}
        viewBox={`0 0 ${gradientLayout.width} ${gradientLayout.height}`}
        className="block overflow-visible"
      >
        <PreviewSvgText
          text={valueText}
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
