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
 * @param {object|null} props.metricPreviewModel - Container-owned preview model, required for non-gradient widgets.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @returns {JSX.Element|null} SVG or div element with metric widget preview, or null.
 */

import { PreviewMetricIcon, PreviewSvgText } from '../../shared/PreviewSvgComponents'
import { useMetricPreviewPresentation } from './useMetricPreview'

function renderMetricTextRuns(textRuns, presentation, sceneStyle) {
  const renderedRuns = []
  for (const run of textRuns) {
    renderedRuns.push(
      <PreviewSvgText
        key={run.key}
        text={run.text}
        x={run.x}
        baseline={run.baseline}
        color={run.color}
        fontFamily={presentation.fontFamily}
        fontSize={run.fontSize}
        opacity={presentation.widgetOpacity}
        shadow={presentation.shadow}
        shadowFilterId={presentation.shadow ? run.shadowFilterId : undefined}
        borderColor={sceneStyle?.border_color}
        borderThickness={sceneStyle?.border_thickness}
      />,
    )
  }
  return renderedRuns
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

  if (presentation.mode === 'metric') {
    return (
      <div
        className="pointer-events-none relative"
        style={{
          width: presentation.visualBounds.width,
          height: presentation.visualBounds.height,
        }}
      >
        <div className="absolute" style={{ width: presentation.visualBounds.width, height: presentation.visualBounds.height }}>
          <svg
            width={presentation.visualBounds.width}
            height={presentation.visualBounds.height}
            viewBox={`0 0 ${presentation.visualBounds.width} ${presentation.visualBounds.height}`}
            className="absolute left-0 top-0 block overflow-visible"
          >
            {presentation.metricLayout.icon && presentation.iconSvg ? (
              <PreviewMetricIcon
                icon={presentation.iconSvg}
                left={presentation.iconLeft}
                top={presentation.iconTop}
                size={presentation.metricLayout.icon.size}
                color={widget.data.icon_color}
                opacity={presentation.widgetOpacity}
                shadow={presentation.shadow}
                shadowFilterId={presentation.shadow ? presentation.iconShadowFilterId : undefined}
              />
            ) : null}
            {renderMetricTextRuns(presentation.textRuns, presentation, sceneStyle)}
          </svg>
        </div>
      </div>
    )
  }

  const gradientValueLeft = presentation.gradientLayout.value.left
  const gradientUnitX = gradientValueLeft + presentation.gradientPrefixWidth

  return (
    <svg
      width={presentation.gradientLayout.width}
      height={presentation.gradientLayout.height}
      viewBox={`0 0 ${presentation.gradientLayout.width} ${presentation.gradientLayout.height}`}
      className="pointer-events-none block overflow-visible"
    >
      {presentation.gradientValuePrefix ? (
        <PreviewSvgText
          text={presentation.gradientValuePrefix}
          x={gradientValueLeft}
          baseline={presentation.gradientLayout.value.baseline}
          color={widget.data.color}
          fontFamily={presentation.fontFamily}
          fontSize={widget.data.font_size}
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
          color={widget.data.unit_color}
          fontFamily={presentation.fontFamily}
          fontSize={widget.data.font_size}
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
