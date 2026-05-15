/**
 * Shared SVG rendering components used across per-widget preview renderers.
 *
 * Each component is a pure presentational SVG fragment that receives
 * all data via props — no hooks, no store access, no side effects.
 */

import { normalizeSvgShadowColor } from '../utils/svgPreviewUtils'

export function PreviewSvgShadowOnlyFilter({ id, shadow, opacity = 1 }) {
  if (!id || !shadow) {
    return null
  }

  const shadowColor = normalizeSvgShadowColor(shadow.color, opacity)

  return (
    <defs>
      <filter id={id} x="-50%" y="-50%" width="200%" height="200%" colorInterpolationFilters="sRGB">
        <feGaussianBlur in="SourceAlpha" stdDeviation={Math.max(shadow.strength, 0)} result="shadow-blur" />
        <feOffset in="shadow-blur" dx={shadow.distance} dy={shadow.distance} result="shadow-offset" />
        <feFlood floodColor={shadowColor.color} floodOpacity={shadowColor.opacity} result="shadow-color" />
        <feComposite in="shadow-color" in2="shadow-offset" operator="in" />
      </filter>
    </defs>
  )
}

export function PreviewSvgShadowBlurFilter({ id, shadow }) {
  if (!id || !shadow || shadow.strength <= 0) {
    return null
  }

  return (
    <defs>
      <filter id={id} x="-50%" y="-50%" width="200%" height="200%" colorInterpolationFilters="sRGB">
        <feGaussianBlur stdDeviation={shadow.strength} />
      </filter>
    </defs>
  )
}

function PreviewSvgIconShadow({ icon, left, top, iconScale, shadow, shadowFilterId, opacity }) {
  if (!shadow || !shadowFilterId) {
    return null
  }

  const shadowColor = normalizeSvgShadowColor(shadow.color, opacity)

  return (
    <g
      transform={`translate(${left + shadow.distance} ${top + shadow.distance}) scale(${iconScale})`}
      fill="none"
      stroke={shadowColor.color}
      strokeWidth={icon.strokeWidth || 2}
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeOpacity={shadowColor.opacity}
      filter={shadow.strength > 0 ? `url(#${shadowFilterId})` : undefined}
      dangerouslySetInnerHTML={{ __html: icon.innerMarkup }}
    />
  )
}

export function PreviewSvgText({
  text,
  x = 0,
  baseline,
  color,
  fontFamily,
  fontSize,
  opacity,
  shadow,
  shadowFilterId,
  borderColor,
  borderThickness,
}) {
  const hasShadow = Boolean(shadow && shadowFilterId)

  return (
    <>
      <PreviewSvgShadowOnlyFilter id={shadowFilterId} shadow={shadow} opacity={opacity} />
      {hasShadow ? (
        <text
          x={x}
          y={baseline}
          fill={color}
          fillOpacity={opacity}
          fontFamily={fontFamily}
          fontSize={fontSize}
          stroke="none"
          filter={`url(#${shadowFilterId})`}
        >
          {text}
        </text>
      ) : null}
      <text
        x={x}
        y={baseline}
        fill={color}
        fontFamily={fontFamily}
        fontSize={fontSize}
        opacity={opacity}
        paintOrder="stroke fill"
        stroke={borderColor || 'none'}
        strokeWidth={borderThickness || 0}
      >
        {text}
      </text>
    </>
  )
}

export function PreviewMetricIcon({ icon, left, top, size, color, opacity, shadow, shadowFilterId }) {
  if (!icon?.innerMarkup || size <= 0) {
    return null
  }

  const iconScale = size / 24
  const normalizedShadow =
    shadow && iconScale > 0
      ? {
          ...shadow,
          strength: shadow.strength / iconScale,
        }
      : null

  return (
    <>
      <PreviewSvgShadowBlurFilter id={shadowFilterId} shadow={normalizedShadow} />
      <PreviewSvgIconShadow
        icon={icon}
        left={left}
        top={top}
        iconScale={iconScale}
        shadow={shadow}
        shadowFilterId={shadowFilterId}
        opacity={opacity}
      />
      <g
        transform={`translate(${left} ${top}) scale(${iconScale})`}
        fill="none"
        stroke={color}
        strokeWidth={icon.strokeWidth || 2}
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity={opacity}
        dangerouslySetInnerHTML={{ __html: icon.innerMarkup }}
      />
    </>
  )
}

export function PreviewPolylineShadow({ points, shadow, blurFilterId, strokeWidth, strokeOpacity, rotation = 0 }) {
  if (!shadow || !points) {
    return null
  }

  const shadowColor = normalizeSvgShadowColor(shadow.color, strokeOpacity)
  const rotationRadians = ((Number(rotation) || 0) * Math.PI) / 180
  const offsetX = (Math.cos(rotationRadians) + Math.sin(rotationRadians)) * shadow.distance
  const offsetY = (Math.cos(rotationRadians) - Math.sin(rotationRadians)) * shadow.distance

  return (
    <polyline
      fill="none"
      stroke={shadowColor.color}
      strokeOpacity={shadowColor.opacity}
      strokeWidth={strokeWidth}
      strokeLinejoin="round"
      strokeLinecap="round"
      points={points}
      transform={`translate(${offsetX} ${offsetY})`}
      filter={shadow.strength > 0 && blurFilterId ? `url(#${blurFilterId})` : undefined}
    />
  )
}

export function PreviewMarkerLayers({ layers, x, y }) {
  if (!layers?.length || !Number.isFinite(x) || !Number.isFinite(y)) {
    return null
  }

  return (
    <>
      {layers.map((layer, index) => (
        <circle
          key={`${layer.radius}-${layer.color}-${index}`}
          cx={x}
          cy={y}
          r={layer.radius}
          fill={layer.solidFill ? layer.color : 'none'}
          stroke={layer.solidFill ? 'none' : layer.color}
          strokeWidth={layer.solidFill ? undefined : Math.min(Math.max(Math.round(layer.radius * 0.18), 1), 3)}
          opacity={layer.opacity}
        />
      ))}
    </>
  )
}
