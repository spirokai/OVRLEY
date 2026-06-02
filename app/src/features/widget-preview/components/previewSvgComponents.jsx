/**
 * Shared SVG rendering components used across per-widget preview renderers.
 *
 * Each component is a pure presentational SVG fragment that receives
 * all data via props — no hooks, no store access, no side effects.
 */

import { normalizeSvgShadowColor } from '../utils/svgPreviewUtils'

/**
 * Defines an SVG filter element for rendering text shadows.
 *
 * Creates a feGaussianBlur + feOffset + feFlood + feComposite chain that
 * produces a drop-shadow effect matching the Skia renderer's shadow output.
 *
 * @param {object} props
 * @param {string} props.id - Unique filter ID referenced by url(#id).
 * @param {object|null} props.shadow - Shadow configuration ({ color, distance, strength }).
 * @param {number} [props.opacity=1] - Opacity multiplier for the shadow color.
 * @returns {JSX.Element|null} SVG defs element, or null if id or shadow is missing.
 */
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

/**
 * Defines an SVG blur filter used for line/polyline shadow effects.
 *
 * Creates a simple feGaussianBlur filter applied to polylines via a separate
 * shadow polyline element rendered behind the main stroke.
 *
 * @param {object} props
 * @param {string} props.id - Unique filter ID referenced by url(#id).
 * @param {object|null} props.shadow - Shadow configuration ({ color, distance, strength }).
 * @returns {JSX.Element|null} SVG defs element, or null if shadow has no strength.
 */
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

/**
 * Renders SVG text with optional shadow and border/stroke styling.
 *
 * Renders two text elements — a shadow layer (filtered) behind the main text layer.
 * The main text uses paintOrder="stroke fill" for the border effect.
 *
 * @param {object} props
 * @param {string} props.text - Text content to render.
 * @param {number} [props.x=0] - X position.
 * @param {number} props.baseline - Y baseline position (text baseline, not top).
 * @param {string} props.color - Text fill color.
 * @param {string} props.fontFamily - Font family.
 * @param {number} props.fontSize - Font size in pixels.
 * @param {number} props.opacity - Text opacity.
 * @param {object|null} props.shadow - Shadow configuration ({ color, distance, strength }).
 * @param {string} [props.shadowFilterId] - Filter ID for the shadow layer.
 * @param {string} [props.borderColor] - Text border/stroke color.
 * @param {number} [props.borderThickness] - Text border/stroke thickness in pixels.
 * @returns {JSX.Element} Fragment containing SVG text elements.
 */
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
  textTransform,
}) {
  const hasShadow = Boolean(shadow && shadowFilterId)
  const textStyle = textTransform ? { textTransform } : undefined

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
          style={textStyle}
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
        style={textStyle}
      >
        {text}
      </text>
    </>
  )
}

/**
 * Renders an SVG metric icon with optional shadow support.
 *
 * The icon is rendered as an SVG group via dangerouslySetInnerHTML, scaled
 * from a 24px base size. Shadow is applied to a separate duplicated icon group
 * rendered behind the main icon, with shadow strength inversely scaled.
 *
 * @param {object} props
 * @param {object} props.icon - Icon definition ({ innerMarkup, strokeWidth }).
 * @param {number} props.left - X position in SVG coordinates.
 * @param {number} props.top - Y position in SVG coordinates.
 * @param {number} props.size - Icon size in pixels (scaled from 24px base).
 * @param {string} props.color - Icon stroke color.
 * @param {number} props.opacity - Icon opacity.
 * @param {object|null} props.shadow - Shadow configuration.
 * @param {string} [props.shadowFilterId] - Filter ID for shadow blur.
 * @returns {JSX.Element|null} SVG groups for icon + shadow, or null if no icon.
 */
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

/**
 * Renders a shadow layer underneath an SVG polyline, accounting for rotation.
 *
 * Creates an offset polyline that mirrors the main route/elevation line, with
 * rotation-adjusted shadow displacement to produce directionally correct shadows.
 *
 * @param {object} props
 * @param {string} props.points - SVG points string (e.g. "x1,y1 x2,y2 ...").
 * @param {object|null} props.shadow - Shadow configuration ({ color, distance, strength }).
 * @param {string} props.blurFilterId - ID of the blur filter to apply.
 * @param {number} props.strokeWidth - Polyline stroke width in pixels.
 * @param {number} props.strokeOpacity - Polyline stroke opacity (0–1).
 * @param {number} [props.rotation=0] - Rotation angle in degrees for shadow offset adjustment.
 * @returns {JSX.Element|null} SVG polyline element, or null if no shadow or points.
 */
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

/**
 * Renders a set of concentric circle marker layers at a given position.
 *
 * Used to indicate the current playhead position along a route or elevation profile.
 * Layers are sorted by descending radius; the innermost layer is rendered with
 * solid fill while outer layers are stroked outlines.
 *
 * @param {object} props
 * @param {Array<{radius: number, color: string, opacity: number, solidFill: boolean, strokeWidth?: number}>} props.layers - Sorted marker layer definitions.
 * @param {number} props.x - X position of the marker center.
 * @param {number} props.y - Y position of the marker center.
 * @returns {JSX.Element|null} Fragment of SVG circle elements, or null if no valid layers/position.
 */
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
          strokeWidth={layer.solidFill ? undefined : (layer.strokeWidth ?? Math.min(Math.max(Math.round(layer.radius * 0.18), 1), 3))}
          opacity={layer.opacity}
        />
      ))}
    </>
  )
}
