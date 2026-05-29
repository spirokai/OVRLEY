/**
 * Renders the heading compass tape widget SVG preview — a horizontal scrolling
 * tape with ticks, labels, and a configurable center indicator.
 *
 * The tape is rendered as an SVG `<pattern>` with `patternUnits="userSpaceOnUse"`
 * and width `360 × pixelsPerDegree` px. Scrolling is achieved via
 * `patternTransform="translate(-offset, 0)"`, mirroring Skia's TileMode::Repeat.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object|null} [props.activity] - Activity data with heading series.
 * @param {number} [props.previewSecond] - Current preview time in seconds.
 * @param {number} [props.globalOpacity] - Global opacity multiplier.
 * @returns {JSX.Element} SVG element for heading widget preview.
 */

import { useMemo } from 'react'
import {
  headingOffset,
  visibleTicks,
  visibleLabels,
  chevronVertices,
  highlightBarMarkerVertices,
} from '../utils/headingGeometry'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'

const DEMO_HEADING = 90

function resolveColor(color, fallback = '#ffffff') {
  return color || fallback
}

function parseOpacity(globalOpacity, widgetOpacity) {
  const base = Number(widgetOpacity) || 1
  return Math.min(1, Math.max(0, base * (Number(globalOpacity) || 1)))
}

/**
 * Interpolates the heading value at a given preview second from the activity data.
 */
function interpolateHeading(activity, previewSecond) {
  if (!activity?.heading?.length || !activity?.sample_elapsed_seconds?.length) {
    return DEMO_HEADING
  }

  const elapsed = activity.sample_elapsed_seconds
  const heading = activity.heading
  const target = Number(previewSecond) || 0

  if (elapsed.length === 1) return heading[0] ?? DEMO_HEADING
  if (target <= elapsed[0]) return heading[0] ?? DEMO_HEADING
  if (target >= elapsed[elapsed.length - 1]) return heading[heading.length - 1] ?? DEMO_HEADING

  for (let i = 0; i < elapsed.length - 1; i++) {
    if (target >= elapsed[i] && target <= elapsed[i + 1]) {
      const t = (target - elapsed[i]) / (elapsed[i + 1] - elapsed[i])
      const a = heading[i] ?? heading[i + 1] ?? DEMO_HEADING
      const b = heading[i + 1] ?? heading[i] ?? DEMO_HEADING
      return a + (b - a) * t
    }
  }

  return DEMO_HEADING
}

/**
 * Renders tick marks into the tape pattern.
 */
function renderTicks(ticks, height, config) {
  const majorLength = (height * (config.major_tick_length_pct ?? 40)) / 100
  const minorLength = (height * (config.minor_tick_length_pct ?? 20)) / 100
  const thickness = config.tick_thickness ?? 2
  const tickColor = resolveColor(config.tick_color)
  const cardinalColor = resolveColor(config.cardinal_tick_color, tickColor)
  const centerY = height / 2

  return ticks.map((tick, i) => {
    const length = tick.isMajor ? majorLength : minorLength
    const top = config.tick_alignment === 'centered' ? centerY - length / 2 : centerY
    const color = tick.isCardinal ? cardinalColor : tickColor

    return (
      <line
        key={`tick-${i}`}
        x1={tick.x}
        y1={top}
        x2={tick.x}
        y2={top + length}
        stroke={color}
        strokeWidth={thickness}
      />
    )
  })
}

/**
 * Renders labels below the ticks.
 */
function renderLabels(labels, height, config) {
  const majorLength = (height * (config.major_tick_length_pct ?? 40)) / 100
  const centerY = height / 2
  const tickBottom = config.tick_alignment === 'centered' ? centerY + majorLength / 2 : centerY + majorLength
  const labelY = tickBottom + (config.label_offset ?? 4) + (config.label_font_size ?? 12)
  const fontSize = config.label_font_size ?? 12
  const numericColor = resolveColor(config.numeric_label_color)
  const cardinalColor = resolveColor(config.cardinal_label_color, numericColor)

  return labels.map((label, i) => (
    <text
      key={`label-${i}`}
      x={label.x}
      y={labelY}
      textAnchor="middle"
      fill={label.isCardinal ? cardinalColor : numericColor}
      fontSize={fontSize}
      fontFamily="Arial, sans-serif"
    >
      {label.text}
    </text>
  ))
}

/**
 * Renders the chevron indicator.
 */
function renderChevron(centerX, topY, bottomY, config, shadowFilterId) {
  const size = config.indicator_size ?? 10
  const color = resolveColor(config.indicator_color)
  const placement = config.indicator_placement ?? 'top'

  const drawOne = (edgeY, pointingDown, key) => {
    const verts = chevronVertices(centerX, edgeY, size, pointingDown)
    const points = verts.map((v) => `${v.x},${v.y}`).join(' ')
    return (
      <polygon
        key={key}
        points={points}
        fill={color}
        filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}
      />
    )
  }

  if (placement === 'top') return drawOne(topY, true, 'chevron-top')
  if (placement === 'bottom') return drawOne(bottomY, false, 'chevron-bottom')
  if (placement === 'both') {
    return (
      <>
        {drawOne(topY, true, 'chevron-top')}
        {drawOne(bottomY, false, 'chevron-bottom')}
      </>
    )
  }
  return null
}

/**
 * Renders the highlight bar indicator.
 */
function renderHighlightBar(centerX, topY, bottomY, height, config, shadowFilterId) {
  const barWidth = config.indicator_size ?? 10
  const barHalfWidth = barWidth / 2
  const color = resolveColor(config.indicator_color)
  const placement = config.indicator_placement ?? 'top'

  const drawMarker = (edgeY, pointingDown, key) => {
    const verts = highlightBarMarkerVertices(centerX, edgeY, barHalfWidth, pointingDown)
    const points = verts.map((v) => `${v.x},${v.y}`).join(' ')
    return (
      <polygon
        key={key}
        points={points}
        fill={color}
        filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}
      />
    )
  }

  return (
    <>
      <rect
        x={centerX - barHalfWidth}
        y={topY}
        width={barWidth}
        height={height}
        fill={color}
        fillOpacity={0.3}
        filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}
      />
      {(placement === 'top' || placement === 'both') && drawMarker(topY, true, 'marker-top')}
      {(placement === 'bottom' || placement === 'both') && drawMarker(bottomY, false, 'marker-bottom')}
    </>
  )
}

/**
 * Builds the shadow filter SVG definition.
 */
function buildShadowFilter(id, shadow) {
  if (!shadow?.color || (!shadow.strength && !shadow.distance)) return null

  return (
    <filter id={id} x="-50%" y="-50%" width="200%" height="200%">
      <feDropShadow
        dx={shadow.distance ?? 0}
        dy={shadow.distance ?? 0}
        stdDeviation={shadow.strength ?? 0}
        floodColor={shadow.color}
        floodOpacity={1}
      />
    </filter>
  )
}

export function OverlayHeadingWidget({ widget, activity, previewSecond, globalOpacity }) {
  const data = widget.data ?? {}
  const width = Math.max(Number(data.width) || 400, 80)
  const height = Math.max(Number(data.height) || 80, 20)
  const ppd = Number(data.pixels_per_degree) || 5
  const opacity = parseOpacity(globalOpacity, data.opacity)
  const tapeWidth = 360 * ppd

  const heading = useMemo(
    () => interpolateHeading(activity, previewSecond),
    [activity, previewSecond]
  )

  const offset = headingOffset(heading, ppd)

  const ticks = useMemo(
    () =>
      visibleTicks(
        0, // heading=0 for the static pattern image
        ppd,
        tapeWidth,
        Number(data.major_tick_interval) || 15,
        Number(data.minor_ticks_per_major) || 3,
        data.show_major_ticks !== false,
        data.show_minor_ticks !== false
      ),
    [ppd, tapeWidth, data.major_tick_interval, data.minor_ticks_per_major, data.show_major_ticks, data.show_minor_ticks]
  )

  const labels = useMemo(
    () => visibleLabels(ticks, data.show_numeric_labels !== false, data.show_cardinal_labels !== false),
    [ticks, data.show_numeric_labels, data.show_cardinal_labels]
  )

  const shadowFilterId = sanitizeSvgId(`${widget.id}-indicator-shadow`)
  const shadow = data.shadow_color
    ? { color: data.shadow_color, distance: Number(data.shadow_distance) || 0, strength: Number(data.shadow_strength) || 0 }
    : null

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block overflow-visible"
      style={{ opacity: opacity < 1 ? opacity : undefined }}
    >
      <defs>
        <pattern
          id={sanitizeSvgId(`${widget.id}-tape-pattern`)}
          patternUnits="userSpaceOnUse"
          width={tapeWidth}
          height={height}
          patternTransform={`translate(${-offset}, 0)`}
        >
          {renderTicks(ticks, height, data)}
          {renderLabels(labels, height, data)}
        </pattern>
        {shadow && buildShadowFilter(shadowFilterId, shadow)}
      </defs>

      {/* Tape background filled with the scrolling pattern */}
      <rect width={width} height={height} fill={`url(#${sanitizeSvgId(`${widget.id}-tape-pattern`)})`} />

      {/* Indicator overlay */}
      {data.show_indicator !== false && (
        <>
          {data.indicator_style === 'highlight_bar'
            ? renderHighlightBar(
                width / 2,
                0,
                height,
                height,
                data,
                shadow ? shadowFilterId : null
              )
            : renderChevron(
                width / 2,
                0,
                height,
                data,
                shadow ? shadowFilterId : null
              )}
        </>
      )}
    </svg>
  )
}
