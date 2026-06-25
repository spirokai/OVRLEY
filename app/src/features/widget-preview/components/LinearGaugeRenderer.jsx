/**
 * Linear gauge SVG renderer — draws the filled track, border, and optional
 * min/max labels using pre-resolved widget data.
 *
 * Stateless: receives a resolved widget data snapshot and renders purely
 * from props. Does not own any activity or variant-resolution logic.
 *
 * @module LinearGaugeRenderer
 */

import { formatLinearGaugeLabel, getLinearGaugeLayout } from '../utils/linearGaugeGeometry'
import { getTextShadowParts } from '../utils/shadowUtils'
import { normalizeSvgShadowColor } from '../utils/svgPreviewUtils'
import { getPreviewFontFamily, measurePreviewText } from '../utils/textMeasurement'
import { getInterpolatedActivityValue, NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT } from '@/features/overlay-editor'
import { useFontMetricsVersion } from '../hooks/useFontMetricsVersion'
import { PreviewSvgShadowBlurFilter, PreviewSvgText } from './previewSvgComponents'
import { useId } from 'react'

const LINEAR_GAUGE_LABEL_GAP_PX = 8

/**
 * Extracts the activity series for the metric this widget displays.
 *
 * @param {object} activity - Activity data with per-metric series arrays.
 * @param {object} widget - Widget config (resolved data).
 * @returns {number[]} The metric series, or empty array.
 */
function seriesForWidget(activity, widget) {
  const key = widget?.data?.value || widget?.type
  return Array.isArray(activity?.[key]) ? activity[key] : []
}

function originXForCenteredText(measurement, centerX) {
  return centerX - ((measurement.boundsLeft ?? 0) + (measurement.boundsRight ?? measurement.width ?? 0)) * 0.5
}

function baselineYForCenteredText(measurement, centerY) {
  return centerY + ((measurement.ascent ?? 0) - (measurement.descent ?? 0)) * 0.5
}

function getLinearGaugeLabelGap(labelFontSize) {
  return Math.max(labelFontSize * 0.35, LINEAR_GAUGE_LABEL_GAP_PX)
}

function getRectCornerRadii(radius, width, height) {
  return { rx: Math.min(radius, width * 0.5), ry: Math.min(radius, height * 0.5) }
}

function getLinearGaugeLabelLayout({ data, width, height, labelFontFamily, labelFontSize, minLabel, maxLabel }) {
  const gap = getLinearGaugeLabelGap(labelFontSize)
  const minMeasure = measurePreviewText(minLabel, labelFontSize, labelFontFamily)
  const maxMeasure = measurePreviewText(maxLabel, labelFontSize, labelFontFamily)
  const fontMetrics = measurePreviewText(NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT, labelFontSize, labelFontFamily)
  const fontAscent = fontMetrics.fontAscent ?? fontMetrics.ascent ?? 0
  const fontDescent = fontMetrics.fontDescent ?? fontMetrics.descent ?? 0

  if (data.orientation === 'vertical') {
    if (data.min_max_label_position === 'right') {
      return {
        min: {
          x: width + gap - (minMeasure.boundsLeft ?? 0),
          y: baselineYForCenteredText(minMeasure, height),
        },
        max: {
          x: width + gap - (maxMeasure.boundsLeft ?? 0),
          y: baselineYForCenteredText(maxMeasure, 0),
        },
      }
    }

    return {
      min: {
        x: -gap - (minMeasure.boundsRight ?? minMeasure.width ?? 0),
        y: baselineYForCenteredText(minMeasure, height),
      },
      max: {
        x: -gap - (maxMeasure.boundsRight ?? maxMeasure.width ?? 0),
        y: baselineYForCenteredText(maxMeasure, 0),
      },
    }
  }

  const baseline = data.min_max_label_position === 'top' ? -gap - fontDescent : height + gap + fontAscent

  return {
    min: {
      x: originXForCenteredText(minMeasure, 0),
      y: baseline,
    },
    max: {
      x: originXForCenteredText(maxMeasure, width),
      y: baseline,
    },
  }
}

export function OverlayLinearGaugeWidget({ widget, activity, previewSecond, globalOpacity = 1, globalScale = 1, sceneStyle }) {
  const data = widget.data
  const maskId = useId()
  const flatFillClipId = `${maskId}-flat-fill`
  const innerTrackClipId = `${maskId}-inner-track`
  const labelFontSize = data.min_max_label_font_size ?? 12
  const labelFontFamily = getPreviewFontFamily(data.min_max_label_font)
  useFontMetricsVersion(labelFontFamily, labelFontSize)
  if (data.display_type !== 'linear') return null

  const width = data.width
  const height = data.height
  const scale = globalScale || 1
  const values = seriesForWidget(activity, widget)
  const value = getInterpolatedActivityValue(activity, data.value || widget.type, previewSecond)
  const borderThickness = data.track_border_thickness ?? 0
  const layout = getLinearGaugeLayout({
    value,
    values,
    width,
    height,
    orientation: data.orientation,
    borderThickness,
  })
  const opacity = (data.opacity ?? 1) * globalOpacity
  const cornerRadius = data.track_corner_radius ?? 0
  const fillCornerRadius = Math.max(0, cornerRadius - borderThickness)
  const showLabels = Boolean(data.show_min_max_labels)
  const minLabel = formatLinearGaugeLabel(layout.min)
  const maxLabel = formatLinearGaugeLabel(layout.max)
  const labelLayout = showLabels ? getLinearGaugeLabelLayout({ data, width, height, labelFontFamily, labelFontSize, minLabel, maxLabel }) : null
  const shadow = getTextShadowParts(sceneStyle)
  const shadowEnabled = borderThickness > 0 && shadow
  const shadowFilterId = shadowEnabled?.strength > 0 ? `linear-gauge-${widget.id || maskId}-shadow` : null
  const fillIsFlat = Boolean(data.track_fill_flat)
  const shadowMaskId = `${maskId}-shadow-mask`
  const trackCornerRadii = getRectCornerRadii(cornerRadius, width, height)
  const innerTrackRect = {
    x: borderThickness,
    y: borderThickness,
    width: Math.max(0, width - borderThickness * 2),
    height: Math.max(0, height - borderThickness * 2),
  }
  const innerTrackCornerRadii = getRectCornerRadii(fillCornerRadius, innerTrackRect.width, innerTrackRect.height)
  const fillCornerRadii = getRectCornerRadii(fillCornerRadius, layout.fillRect.width, layout.fillRect.height)
  const shadowColor = shadowEnabled ? normalizeSvgShadowColor(shadowEnabled.color, opacity) : null
  const outerShadow =
    shadowColor != null ? (
      <g transform={`translate(${shadowEnabled.distance} ${shadowEnabled.distance})`} filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}>
        <rect
          x={0}
          y={0}
          width={width}
          height={height}
          rx={trackCornerRadii.rx}
          ry={trackCornerRadii.ry}
          fill={shadowColor.color}
          opacity={shadowColor.opacity}
          mask={borderThickness > 0 ? `url(#${shadowMaskId})` : undefined}
        />
      </g>
    ) : null
  const useRoundedFill = fillCornerRadius > 0
  const filledTrack =
    useRoundedFill && fillIsFlat ? (
      <rect
        x={innerTrackRect.x}
        y={innerTrackRect.y}
        width={innerTrackRect.width}
        height={innerTrackRect.height}
        rx={innerTrackCornerRadii.rx}
        ry={innerTrackCornerRadii.ry}
        clipPath={`url(#${flatFillClipId})`}
        fill={data.track_filled_color}
        fillOpacity={data.track_filled_opacity}
        opacity={opacity}
      />
    ) : useRoundedFill ? (
      <rect
        x={layout.fillRect.x}
        y={layout.fillRect.y}
        width={layout.fillRect.width}
        height={layout.fillRect.height}
        rx={fillCornerRadii.rx}
        ry={fillCornerRadii.ry}
        clipPath={`url(#${innerTrackClipId})`}
        fill={data.track_filled_color}
        fillOpacity={data.track_filled_opacity}
        opacity={opacity}
      />
    ) : (
      <rect
        x={layout.fillRect.x}
        y={layout.fillRect.y}
        width={layout.fillRect.width}
        height={layout.fillRect.height}
        rx={fillCornerRadii.rx}
        ry={fillCornerRadii.ry}
        fill={data.track_filled_color}
        fillOpacity={data.track_filled_opacity}
        opacity={opacity}
      />
    )

  return (
    <svg
      width={width * scale}
      height={height * scale}
      viewBox={`0 0 ${width} ${height}`}
      className="block overflow-visible"
      data-testid="linear-gauge-preview"
    >
      {shadowFilterId ? <PreviewSvgShadowBlurFilter id={shadowFilterId} shadow={shadowEnabled} /> : null}
      {borderThickness > 0 || useRoundedFill ? (
        <defs>
          {borderThickness > 0 ? (
            <>
              <mask id={maskId}>
                <rect x={0} y={0} width={width} height={height} rx={trackCornerRadii.rx} ry={trackCornerRadii.ry} fill="white" />
                <rect
                  x={innerTrackRect.x}
                  y={innerTrackRect.y}
                  width={innerTrackRect.width}
                  height={innerTrackRect.height}
                  rx={innerTrackCornerRadii.rx}
                  ry={innerTrackCornerRadii.ry}
                  fill="black"
                />
              </mask>
              <mask id={shadowMaskId}>
                <rect x={0} y={0} width={width} height={height} rx={trackCornerRadii.rx} ry={trackCornerRadii.ry} fill="white" />
                <rect
                  x={innerTrackRect.x}
                  y={innerTrackRect.y}
                  width={innerTrackRect.width}
                  height={innerTrackRect.height}
                  rx={innerTrackCornerRadii.rx}
                  ry={innerTrackCornerRadii.ry}
                  fill="black"
                />
              </mask>
            </>
          ) : null}
          {useRoundedFill ? (
            <>
              <clipPath id={flatFillClipId}>
                <rect x={layout.fillRect.x} y={layout.fillRect.y} width={layout.fillRect.width} height={layout.fillRect.height} />
              </clipPath>
              <clipPath id={innerTrackClipId}>
                <rect
                  x={innerTrackRect.x}
                  y={innerTrackRect.y}
                  width={innerTrackRect.width}
                  height={innerTrackRect.height}
                  rx={innerTrackCornerRadii.rx}
                  ry={innerTrackCornerRadii.ry}
                />
              </clipPath>
            </>
          ) : null}
        </defs>
      ) : null}
      {outerShadow}
      {borderThickness > 0 ? (
        <rect
          x={0}
          y={0}
          width={width}
          height={height}
          rx={trackCornerRadii.rx}
          ry={trackCornerRadii.ry}
          fill={data.track_border_color}
          mask={`url(#${maskId})`}
          opacity={opacity}
        />
      ) : (
        <rect
          x={0}
          y={0}
          width={width}
          height={height}
          rx={trackCornerRadii.rx}
          ry={trackCornerRadii.ry}
          fill={data.track_empty_color}
          fillOpacity={data.track_empty_opacity}
          opacity={opacity}
        />
      )}
      {borderThickness > 0 ? (
        <rect
          x={innerTrackRect.x}
          y={innerTrackRect.y}
          width={innerTrackRect.width}
          height={innerTrackRect.height}
          rx={innerTrackCornerRadii.rx}
          ry={innerTrackCornerRadii.ry}
          fill={data.track_empty_color}
          fillOpacity={data.track_empty_opacity}
          opacity={opacity}
        />
      ) : null}
      {filledTrack}
      {showLabels ? (
        <>
          <PreviewSvgText
            text={minLabel}
            x={labelLayout.min.x}
            baseline={labelLayout.min.y}
            color={data.min_max_label_color}
            fontFamily={labelFontFamily}
            fontSize={labelFontSize}
            opacity={1}
          />
          <PreviewSvgText
            text={maxLabel}
            x={labelLayout.max.x}
            baseline={labelLayout.max.y}
            color={data.min_max_label_color}
            fontFamily={labelFontFamily}
            fontSize={labelFontSize}
            opacity={1}
          />
        </>
      ) : null}
    </svg>
  )
}
