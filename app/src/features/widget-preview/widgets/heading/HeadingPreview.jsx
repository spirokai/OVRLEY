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

import { WIDGET_FONT_WEIGHT } from '@/lib/widget/standard-widgets'
import { chevronVertices, headingLabelBaseline, headingTickPosition } from './geometry'
import { useHeadingPreviewModel } from './useHeadingPreview'
import { normalizeSvgShadowColor } from '../../shared/svgPreviewUtils'

function renderTicks(ticks, topY, height, config) {
  const elements = []
  for (let index = 0; index < ticks.length; index += 1) {
    const tick = ticks[index]
    const { length, top } = headingTickPosition(height, config, tick.isMajor)
    const y1 = topY + top
    const color = tick.isCardinal ? config.cardinal_tick_color : config.tick_color
    const thickness = tick.isMajor ? config.major_tick_thickness : config.minor_tick_thickness

    elements.push(<line key={`tick-${index}`} x1={tick.x} y1={y1} x2={tick.x} y2={y1 + length} stroke={color} strokeWidth={thickness} />)
  }
  return elements
}

function renderLabels(labels, topY, height, config, fontFamily) {
  const labelY = topY + headingLabelBaseline(height, config)

  const elements = []
  for (let index = 0; index < labels.length; index += 1) {
    const label = labels[index]
    elements.push(
      <text
        key={`label-${index}`}
        x={label.x}
        y={labelY}
        textAnchor="middle"
        fill={label.isMajorLabel ? config.cardinal_label_color : config.label_color}
        fontSize={config.label_font_size}
        fontFamily={fontFamily}
        fontWeight={WIDGET_FONT_WEIGHT}
        style={{ fontOpticalSizing: 'none', fontSynthesis: 'none' }}
      >
        {label.text}
      </text>,
    )
  }
  return elements
}

function ChevronPolygon({ centerX, edgeY, size, pointingDown, color, shadowFilterId }) {
  const pointParts = []
  for (const vertex of chevronVertices(centerX, edgeY, size, pointingDown)) pointParts.push(`${vertex.x},${vertex.y}`)

  return <polygon points={pointParts.join(' ')} fill={color} filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined} />
}

function renderChevron(centerX, topY, bottomY, config, shadowFilterId) {
  if (config.indicator_placement === 'top') {
    return (
      <ChevronPolygon
        centerX={centerX}
        edgeY={topY}
        size={config.indicator_size}
        pointingDown
        color={config.indicator_color}
        shadowFilterId={shadowFilterId}
      />
    )
  }
  if (config.indicator_placement === 'bottom') {
    return (
      <ChevronPolygon
        centerX={centerX}
        edgeY={bottomY}
        size={config.indicator_size}
        pointingDown={false}
        color={config.indicator_color}
        shadowFilterId={shadowFilterId}
      />
    )
  }
  if (config.indicator_placement === 'both') {
    return (
      <>
        <ChevronPolygon
          centerX={centerX}
          edgeY={topY}
          size={config.indicator_size}
          pointingDown
          color={config.indicator_color}
          shadowFilterId={shadowFilterId}
        />
        <ChevronPolygon
          centerX={centerX}
          edgeY={bottomY}
          size={config.indicator_size}
          pointingDown={false}
          color={config.indicator_color}
          shadowFilterId={shadowFilterId}
        />
      </>
    )
  }
  return null
}

function renderHighlightBar(centerX, topY, height, config) {
  return (
    <rect
      x={centerX - config.indicator_size / 2}
      y={topY}
      width={config.indicator_size}
      height={height}
      fill={config.indicator_color}
      fillOpacity={0.3}
    />
  )
}

function HeadingShadowFilter({ id, shadow }) {
  const shadowColor = normalizeSvgShadowColor(shadow.color, 1)

  return (
    <filter id={id} x="-50%" y="-50%" width="200%" height="200%" overflow="visible" colorInterpolationFilters="sRGB">
      <feDropShadow
        dx={shadow.distance}
        dy={shadow.distance}
        stdDeviation={shadow.strength}
        floodColor={shadowColor.color}
        floodOpacity={shadowColor.opacity}
      />
    </filter>
  )
}

function HeadingTapeCopies({ model, config, filterId }) {
  return (
    <g clipPath={`url(#${model.clipPathId})`} filter={filterId ? `url(#${filterId})` : undefined}>
      <g transform={`translate(${-model.wrappedOffset}, 0)`}>
        {renderTicks(model.ticks, model.bodyY, model.tickScaleHeight, config)}
        {renderLabels(model.labels, model.bodyY, model.tickScaleHeight, config, model.labelFontFamily)}
      </g>
      <g transform={`translate(${-model.wrappedOffset + model.tapeWidth}, 0)`}>
        {renderTicks(model.ticks, model.bodyY, model.tickScaleHeight, config)}
        {renderLabels(model.labels, model.bodyY, model.tickScaleHeight, config, model.labelFontFamily)}
      </g>
    </g>
  )
}

export function OverlayHeadingWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneFont, valueFont, sceneStyle }) {
  const model = useHeadingPreviewModel({
    widget,
    activity,
    previewSecond,
    globalOpacity,
    globalScale,
    sceneFont,
    valueFont,
    sceneStyle,
  })

  return (
    <svg
      width={model.displayWidth}
      height={model.displayHeight}
      viewBox={`0 0 ${widget.data.width} ${model.totalHeight}`}
      className="block h-full w-full"
      style={{ opacity: model.opacity < 1 ? model.opacity : undefined }}
    >
      <defs>
        <clipPath id={model.clipPathId}>
          <rect y={model.bodyY} width={widget.data.width} height={model.bodyHeight} />
        </clipPath>
        {model.shadow ? <HeadingShadowFilter id={model.shadowFilterId} shadow={model.shadow} /> : null}
      </defs>

      {model.shadow ? <HeadingTapeCopies model={model} config={widget.data} filterId={model.shadowFilterId} /> : null}

      <HeadingTapeCopies model={model} config={widget.data} />

      {widget.data.show_indicator && (
        <>
          {widget.data.indicator_style === 'highlight_bar'
            ? renderHighlightBar(widget.data.width / 2, 0, model.totalHeight, widget.data)
            : renderChevron(widget.data.width / 2, 0, model.totalHeight, widget.data, model.shadow ? model.shadowFilterId : null)}
        </>
      )}
    </svg>
  )
}
