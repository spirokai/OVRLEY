/**
 * Renders the heading compass tape widget SVG preview â€” a horizontal scrolling
 * tape with ticks, labels, and a configurable center indicator.
 *
 * Receives resolved data from resolveActiveMetricWidgetData, which guarantees
 * all fields are present including frame geometry â€” no defensive fallback
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

import { chevronVertices, headingLabelBaseline, headingTickPosition } from '../utils/headingGeometry'
import { useHeadingPreviewModel } from '../hooks/useHeadingPreviewModel'

function renderTicks(ticks, topY, height, config) {
  const majorThickness = config.major_tick_thickness
  const minorThickness = config.minor_tick_thickness
  const tickColor = config.tick_color
  const cardinalColor = config.cardinal_tick_color

  return ticks.map((tick, i) => {
    const { length, top } = headingTickPosition(height, config, tick.isMajor)
    const y1 = topY + top
    const color = tick.isCardinal ? cardinalColor : tickColor
    const thickness = tick.isMajor ? majorThickness : minorThickness

    return <line key={`tick-${i}`} x1={tick.x} y1={y1} x2={tick.x} y2={y1 + length} stroke={color} strokeWidth={thickness} />
  })
}

function renderLabels(labels, topY, height, config, fontFamily) {
  const labelY = topY + headingLabelBaseline(height, config)
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
  if (!shadow?.color || shadow.strength <= 0) return null

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

export function OverlayHeadingWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneFont, valueFont, sceneStyle }) {
  const model = useHeadingPreviewModel({ widget, activity, previewSecond, globalOpacity, globalScale, sceneFont, valueFont, sceneStyle })
  const { data } = model

  const renderTapeCopies = (filterId = null) => (
    <g clipPath={`url(#${model.clipPathId})`} filter={filterId ? `url(#${filterId})` : undefined}>
      <g transform={`translate(${-model.wrappedOffset}, 0)`}>
        {renderTicks(model.ticks, model.bodyY, model.tickScaleHeight, data)}
        {renderLabels(model.labels, model.bodyY, model.tickScaleHeight, data, model.labelFontFamily)}
      </g>
      <g transform={`translate(${-model.wrappedOffset + model.tapeWidth}, 0)`}>
        {renderTicks(model.ticks, model.bodyY, model.tickScaleHeight, data)}
        {renderLabels(model.labels, model.bodyY, model.tickScaleHeight, data, model.labelFontFamily)}
      </g>
    </g>
  )

  return (
    <svg
      width={model.displayWidth}
      height={model.displayHeight}
      viewBox={`0 0 ${model.width} ${model.totalHeight}`}
      className="block h-full w-full"
      style={{ opacity: model.opacity < 1 ? model.opacity : undefined }}
    >
      <defs>
        <clipPath id={model.clipPathId}>
          <rect y={model.bodyY} width={model.width} height={model.bodyHeight} />
        </clipPath>
        {model.shadow && buildShadowFilter(model.shadowFilterId, model.shadow)}
      </defs>

      {model.shadow?.strength > 0 ? renderTapeCopies(model.shadowFilterId) : null}

      {renderTapeCopies()}

      {data.show_indicator && (
        <>
          {data.indicator_style === 'highlight_bar'
            ? renderHighlightBar(model.width / 2, model.bodyY, model.bodyHeight, data)
            : renderChevron(model.width / 2, 0, model.totalHeight, data, model.shadow?.strength > 0 ? model.shadowFilterId : null)}
        </>
      )}
    </svg>
  )
}
