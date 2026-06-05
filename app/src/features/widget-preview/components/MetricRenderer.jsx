/**
 * Renders the overlay metric/gradient widget SVG preview â€” value text,
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

import { PreviewMetricIcon, PreviewSvgText } from './previewSvgComponents'
import { useMetricPreviewPresentation } from '../hooks/useMetricPreviewPresentation'

let gradientMeasureTextNode = null

function measureGradientSvgTextWidth(text, fontFamily, fontSize) {
  if (!text || typeof document === 'undefined' || typeof document.createElementNS !== 'function') {
    return 0
  }

  if (!gradientMeasureTextNode) {
    const namespace = 'http://www.w3.org/2000/svg'
    const svg = document.createElementNS(namespace, 'svg')
    gradientMeasureTextNode = document.createElementNS(namespace, 'text')
    svg.style.position = 'absolute'
    svg.style.visibility = 'hidden'
    svg.style.pointerEvents = 'none'
    svg.style.left = '-9999px'
    svg.style.top = '-9999px'
    svg.appendChild(gradientMeasureTextNode)
    document.body?.appendChild(svg)
  }

  gradientMeasureTextNode.setAttribute('font-family', fontFamily)
  gradientMeasureTextNode.setAttribute('font-size', `${fontSize}`)
  gradientMeasureTextNode.textContent = text
  return gradientMeasureTextNode.getComputedTextLength?.() ?? 0
}

export function OverlayMetricWidget({ widget, activity, previewSecond, globalOpacity, globalScale, metricPreviewModel, sceneStyle }) {
  const presentation = useMetricPreviewPresentation({
    widget,
    activity,
    previewSecond,
    globalOpacity,
    globalScale,
    metricPreviewModel,
    sceneStyle,
  })

  if (presentation.mode === 'metric' && presentation.metricLayout) {
    return (
      <div
        className="relative"
        style={{
          width: presentation.renderWidth,
          height: presentation.renderHeight,
        }}
      >
        <div className="absolute" style={{ width: presentation.renderWidth, height: presentation.renderHeight }}>
          <svg
            width={presentation.renderWidth}
            height={presentation.renderHeight}
            viewBox={`0 0 ${presentation.renderWidth} ${presentation.renderHeight}`}
            className="absolute left-0 top-0 block overflow-visible"
          >
            {presentation.metricLayout.icon && presentation.iconSvg ? (
              <PreviewMetricIcon
                icon={presentation.iconSvg}
                left={presentation.iconLeft}
                top={presentation.iconTop}
                size={presentation.metricLayout.icon.size}
                color={widget.data.icon_color || '#40e0d0'}
                opacity={presentation.widgetOpacity}
                shadow={presentation.shadow}
                shadowFilterId={presentation.shadow ? presentation.iconShadowFilterId : undefined}
              />
            ) : null}
            <PreviewSvgText
              text={presentation.valueText}
              x={presentation.metricLayout.value.left + presentation.contentOffsetX}
              baseline={presentation.metricLayout.value.baseline + presentation.contentOffsetY}
              color={presentation.color}
              fontFamily={presentation.fontFamily}
              fontSize={presentation.fontSize}
              opacity={presentation.widgetOpacity}
              shadow={presentation.shadow}
              shadowFilterId={presentation.valueShadowFilterId}
              borderColor={sceneStyle?.border_color}
              borderThickness={sceneStyle?.border_thickness}
            />
            {presentation.metricLayout.units ? (
              <PreviewSvgText
                text={presentation.unitText}
                x={presentation.metricLayout.units.left + presentation.contentOffsetX}
                baseline={presentation.metricLayout.units.baseline + presentation.contentOffsetY}
                color={presentation.unitColor}
                fontFamily={presentation.fontFamily}
                fontSize={presentation.metricLayout.units.fontSize}
                opacity={presentation.widgetOpacity}
                shadow={presentation.shadow}
                shadowFilterId={presentation.unitsShadowFilterId}
                borderColor={sceneStyle?.border_color}
                borderThickness={sceneStyle?.border_thickness}
              />
            ) : null}
          </svg>
        </div>
      </div>
    )
  }

  if (presentation.mode === 'gradient' && presentation.gradientLayout) {
    const renderedGradientText = `${presentation.gradientValuePrefix}${presentation.gradientUnitSuffix}`
    const renderedGradientWidth = measureGradientSvgTextWidth(renderedGradientText, presentation.fontFamily, presentation.fontSize)
    const renderedGradientPrefixWidth = measureGradientSvgTextWidth(presentation.gradientValuePrefix, presentation.fontFamily, presentation.fontSize)
    const gradientValueLeft =
      renderedGradientWidth > 0 ? (presentation.gradientLayout.width - renderedGradientWidth) / 2 : presentation.gradientLayout.value.left
    const gradientUnitX =
      renderedGradientPrefixWidth > 0 ? gradientValueLeft + renderedGradientPrefixWidth : presentation.gradientLayout.value.left + presentation.gradientPrefixWidth

    return (
      <svg
        width={presentation.gradientLayout.width}
        height={presentation.gradientLayout.height}
        viewBox={`0 0 ${presentation.gradientLayout.width} ${presentation.gradientLayout.height}`}
        className="block overflow-visible"
      >
        {presentation.gradientValuePrefix ? (
          <PreviewSvgText
            text={presentation.gradientValuePrefix}
            x={gradientValueLeft}
            baseline={presentation.gradientLayout.value.baseline}
            color={presentation.color}
            fontFamily={presentation.fontFamily}
            fontSize={presentation.fontSize}
            opacity={presentation.widgetOpacity}
            shadow={presentation.shadow}
            shadowFilterId={presentation.valueShadowFilterId}
            borderColor={sceneStyle?.border_color}
            borderThickness={sceneStyle?.border_thickness}
          />
        ) : null}
        {presentation.gradientUnitSuffix ? (
          <PreviewSvgText
            text={presentation.gradientUnitSuffix}
            x={gradientUnitX}
            baseline={presentation.gradientLayout.value.baseline}
            color={presentation.unitColor}
            fontFamily={presentation.fontFamily}
            fontSize={presentation.fontSize}
            opacity={presentation.widgetOpacity}
            shadow={presentation.shadow}
            shadowFilterId={presentation.unitShadowFilterId}
            borderColor={sceneStyle?.border_color}
            borderThickness={sceneStyle?.border_thickness}
          />
        ) : null}
        {presentation.gradientLayout.triangle ? (
          presentation.gradientLayout.triangle.isZero ? (
            <line
              x1={presentation.gradientLayout.triangle.left}
              y1={presentation.gradientLayout.triangle.baseline}
              x2={presentation.gradientLayout.triangle.left + presentation.gradientLayout.triangle.width}
              y2={presentation.gradientLayout.triangle.baseline}
              stroke={presentation.positiveTriangleColor}
              strokeWidth={presentation.gradientZeroLineWidth}
              opacity={presentation.widgetOpacity}
              strokeLinecap="round"
            />
          ) : presentation.trianglePath ? (
            <path
              d={presentation.trianglePath}
              transform={`translate(${presentation.gradientLayout.triangle.left} ${presentation.gradientLayout.triangle.baseline})`}
              fill={presentation.currentGradientValue < 0 ? presentation.negativeTriangleColor : presentation.positiveTriangleColor}
              opacity={presentation.widgetOpacity}
            />
          ) : null
        ) : null}
      </svg>
    )
  }

  return null
}
