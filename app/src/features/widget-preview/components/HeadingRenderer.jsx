/**
 * Renders the heading compass tape widget SVG preview — a horizontal scrolling
 * tape with ticks, labels, and a configurable center indicator.
 *
 * Receives resolved data from resolveActiveMetricWidgetData, which guarantees
 * all fields are present including frame geometry — no defensive fallback
 * values are needed. Viewport minimums are raster constraints, not defaults.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object|null} [props.activity] - Activity data with heading series.
 * @param {number} [props.previewSecond] - Current preview time in seconds.
 * @param {number} [props.globalOpacity] - Global opacity multiplier.
 * @param {number} [props.globalScale] - Global scale multiplier.
 * @param {object} [props.sceneStyle] - Scene style object (shadow, border).
 * @returns {JSX.Element} SVG element for heading widget preview.
 */

import { useMemo } from 'react'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'
import { getPreviewFontFamily, getWidgetOpacity } from '../utils/textMeasurement'
import { headingOffset, visibleTicks, visibleLabels, chevronVertices } from '../utils/headingGeometry'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'
import { getTextShadowParts } from '../utils/shadowUtils'

function renderTicks(ticks, height, config) {
  const majorLength = (height * config.major_tick_length_pct) / 100
  const minorLength = (height * config.minor_tick_length_pct) / 100
  const majorThickness = config.major_tick_thickness
  const minorThickness = config.minor_tick_thickness
  const tickColor = config.tick_color
  const cardinalColor = config.cardinal_tick_color
  const centerY = height / 2

  return ticks.map((tick, i) => {
    const length = tick.isMajor ? majorLength : minorLength
    const top = config.tick_alignment === 'centered' ? centerY - length / 2 : centerY
    const color = tick.isCardinal ? cardinalColor : tickColor
    const thickness = tick.isMajor ? majorThickness : minorThickness

    return <line key={`tick-${i}`} x1={tick.x} y1={top} x2={tick.x} y2={top + length} stroke={color} strokeWidth={thickness} />
  })
}

function renderLabels(labels, height, config, fontFamily) {
  const majorLength = (height * config.major_tick_length_pct) / 100
  const centerY = height / 2
  const tickBottom = config.tick_alignment === 'centered' ? centerY + majorLength / 2 : centerY + majorLength
  const labelY = tickBottom + config.label_offset + config.label_font_size
  const fontSize = config.label_font_size
  const labelColor = config.label_color
  const cardinalColor = config.cardinal_label_color

  return labels.map((label, i) => (
    <text
      key={`label-${i}`}
      x={label.x}
      y={labelY}
      textAnchor="middle"
      fill={label.isMajorLabel ? cardinalColor : labelColor}
      fontSize={fontSize}
      fontFamily={fontFamily}
    >
      {label.text}
    </text>
  ))
}

function renderChevron(centerX, topY, bottomY, config, shadowFilterId) {
  const size = config.indicator_size
  const color = config.indicator_color
  const placement = config.indicator_placement

  const drawOne = (edgeY, pointingDown, key) => {
    const verts = chevronVertices(centerX, edgeY, size, pointingDown)
    const points = verts.map((v) => `${v.x},${v.y}`).join(' ')
    return <polygon key={key} points={points} fill={color} filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined} />
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

function renderHighlightBar(centerX, topY, height, config) {
  const barWidth = config.indicator_size
  const barHalfWidth = barWidth / 2
  const color = config.indicator_color

  return <rect x={centerX - barHalfWidth} y={topY} width={barWidth} height={height} fill={color} fillOpacity={0.3} />
}

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

export function OverlayHeadingWidget({ widget, activity, previewSecond, globalOpacity, sceneFont, valueFont, sceneStyle }) {
  const data = widget.data ?? {}
  const width = Math.max(data.width, 80)
  const height = Math.max(data.height, 20)
  const ppd = data.pixels_per_degree
  const opacity = getWidgetOpacity(data, globalOpacity)
  const tapeWidth = 360 * ppd
  const labelFontSize = data.label_font_size
  const labelFontFamily = getPreviewFontFamily(data.label_font || data.label_font_family || valueFont || sceneFont)
  useFontMetricsVersion(labelFontFamily, labelFontSize)

  const heading = getInterpolatedActivityValue(activity, 'heading', previewSecond)

  const offset = headingOffset(heading, ppd, width)
  const wrappedOffset = ((offset % tapeWidth) + tapeWidth) % tapeWidth

  const ticks = useMemo(
    () => visibleTicks(0, ppd, tapeWidth, data.major_tick_interval, data.minor_ticks_per_major, data.show_major_ticks, data.show_minor_ticks),
    [ppd, tapeWidth, data.major_tick_interval, data.minor_ticks_per_major, data.show_major_ticks, data.show_minor_ticks],
  )

  const labels = useMemo(
    () => visibleLabels(ticks, data.show_minor_labels, data.show_major_labels),
    [ticks, data.show_minor_labels, data.show_major_labels],
  )

  const shadow = useMemo(() => getTextShadowParts(sceneStyle), [sceneStyle])

  const shadowFilterId = sanitizeSvgId(`${widget.id}-shadow`)
  const clipPathId = sanitizeSvgId(`${widget.id}-clip`)

  const renderTapeCopies = (filterId = null) => (
    <g clipPath={`url(#${clipPathId})`} filter={filterId ? `url(#${filterId})` : undefined}>
      <g transform={`translate(${-wrappedOffset}, 0)`}>
        {renderTicks(ticks, height, data)}
        {renderLabels(labels, height, data, labelFontFamily)}
      </g>
      <g transform={`translate(${-wrappedOffset + tapeWidth}, 0)`}>
        {renderTicks(ticks, height, data)}
        {renderLabels(labels, height, data, labelFontFamily)}
      </g>
    </g>
  )

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block h-full w-full"
      style={{ opacity: opacity < 1 ? opacity : undefined }}
    >
      <defs>
        <clipPath id={clipPathId}>
          <rect width={width} height={height} />
        </clipPath>
        {shadow && buildShadowFilter(shadowFilterId, shadow)}
      </defs>

      {shadow && renderTapeCopies(shadowFilterId)}

      {renderTapeCopies()}

      {data.show_indicator && (
        <>
          {data.indicator_style === 'highlight_bar'
            ? renderHighlightBar(width / 2, 0, height, data)
            : renderChevron(width / 2, 0, height, data, shadow ? shadowFilterId : null)}
        </>
      )}
    </svg>
  )
}
