import { PreviewSvgShadowBlurFilter, PreviewSvgShadowOnlyFilter } from '../../shared/PreviewSvgComponents'
import { useGForcePreviewModel } from './useGForcePreview'

function GForceLabel({ model, config }) {
  const label = (
    <>
      <tspan x={model.labelX}>{model.valueText}</tspan>
      {model.unitText ? (
        <>
          {' '}
          <tspan data-testid="g-force-unit" x={model.unitX} fill={config.label_unit_color}>
            {model.unitText}
          </tspan>
        </>
      ) : null}
    </>
  )

  return (
    <>
      <PreviewSvgShadowOnlyFilter id={model.labelShadowFilterId} shadow={model.shadow} opacity={model.opacity} />
      {model.shadow ? (
        <text
          x={model.labelX}
          y={model.labelBaseline}
          fill={model.shadow.color}
          fontFamily={model.fontFamily}
          fontSize={config.label_font_size}
          opacity={model.opacity}
          filter={`url(#${model.labelShadowFilterId})`}
        >
          <tspan x={model.labelX}>{model.valueText}</tspan>
          {model.unitText ? <tspan x={model.unitX}>{model.unitText}</tspan> : null}
        </text>
      ) : null}
      <text
        data-testid="g-force-label"
        x={model.labelX}
        y={model.labelBaseline}
        fill={config.label_color}
        fontFamily={model.fontFamily}
        fontSize={config.label_font_size}
        opacity={model.opacity}
      >
        {label}
      </text>
    </>
  )
}

export function OverlayGForceWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle }) {
  const model = useGForcePreviewModel({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle })
  const config = widget.data

  return (
    <svg
      width={config.width * globalScale}
      height={config.height * globalScale}
      viewBox={`0 0 ${config.width} ${config.height}`}
      className="block overflow-visible"
      data-testid="g-force-preview"
    >
      {model.shadow && config.border_thickness > 0 ? (
        <>
          <PreviewSvgShadowBlurFilter id={model.borderShadowFilterId} shadow={model.shadow} />
          <circle
            cx={model.centerX}
            cy={model.centerY}
            r={model.borderRadius}
            fill="none"
            stroke={model.shadow.color}
            strokeWidth={config.border_thickness}
            opacity={model.opacity}
            transform={`translate(${model.shadow.distance} ${model.shadow.distance})`}
            filter={`url(#${model.borderShadowFilterId})`}
          />
        </>
      ) : null}
      {config.border_thickness > 0 ? (
        <circle
          data-testid="g-force-border"
          cx={model.centerX}
          cy={model.centerY}
          r={model.borderRadius}
          fill="none"
          stroke={config.border_color}
          strokeWidth={config.border_thickness}
          strokeOpacity={config.border_opacity * model.opacity}
        />
      ) : null}
      <defs>
        <clipPath id={model.guideClipId}>
          <circle cx={model.centerX} cy={model.centerY} r={model.innerRadius} />
        </clipPath>
      </defs>
      <circle
        data-testid="g-force-parent-circle"
        cx={model.centerX}
        cy={model.centerY}
        r={model.innerRadius}
        fill={config.fill_color}
        fillOpacity={config.fill_opacity * model.opacity}
      />
      <g data-testid="g-force-static-guides" clipPath={`url(#${model.guideClipId})`} opacity={model.guideOpacity * model.opacity}>
        <line
          data-testid="g-force-horizontal-guide"
          x1={model.centerX - model.radius}
          y1={model.centerY}
          x2={model.centerX + model.radius}
          y2={model.centerY}
          stroke={model.guideColor}
          strokeWidth={model.guideThickness}
        />
        <line
          data-testid="g-force-vertical-guide"
          x1={model.centerX}
          y1={model.centerY - model.radius}
          x2={model.centerX}
          y2={model.centerY + model.radius}
          stroke={model.guideColor}
          strokeWidth={model.guideThickness}
        />
        {model.guideRadii.map((guideRadius) => (
          <circle
            key={guideRadius}
            data-testid={`g-force-guide-circle-${guideRadius}`}
            cx={model.centerX}
            cy={model.centerY}
            r={guideRadius}
            fill="none"
            stroke={model.guideColor}
            strokeWidth={model.guideThickness}
          />
        ))}
      </g>
      <circle
        data-testid="g-force-marker"
        cx={model.markerX}
        cy={model.markerY}
        r={model.markerRadius}
        fill={config.marker_color}
        fillOpacity={config.marker_opacity * model.opacity}
      />
      <GForceLabel model={model} config={config} />
    </svg>
  )
}
