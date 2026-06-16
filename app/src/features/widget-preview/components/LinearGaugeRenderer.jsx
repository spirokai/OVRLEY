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
import { getPreviewFontFamily } from '../utils/textMeasurement'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'
import { PreviewSvgShadowOnlyFilter } from './previewSvgComponents'
import { useId } from 'react'

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

export function OverlayLinearGaugeWidget({ widget, activity, previewSecond, globalOpacity = 1, globalScale = 1, sceneStyle }) {
  const data = widget.data
  const maskId = useId()
  const flatFillClipId = `${maskId}-flat-fill`
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
  const labelFontSize = data.min_max_label_font_size ?? 12
  const labelFontFamily = getPreviewFontFamily(data.min_max_label_font)
  const shadow = getTextShadowParts(sceneStyle)
  const shadowFilterId = shadow ? `linear-gauge-${widget.id || maskId}-shadow` : null
  const fillIsFlat = Boolean(data.track_fill_flat)
  const innerTrackRect = {
    x: borderThickness,
    y: borderThickness,
    width: Math.max(0, width - borderThickness * 2),
    height: Math.max(0, height - borderThickness * 2),
  }
  const filledTrack =
    fillIsFlat && fillCornerRadius > 0 ? (
      <rect
        x={innerTrackRect.x}
        y={innerTrackRect.y}
        width={innerTrackRect.width}
        height={innerTrackRect.height}
        rx={fillCornerRadius}
        ry={fillCornerRadius}
        clipPath={`url(#${flatFillClipId})`}
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
        rx={fillCornerRadius}
        ry={fillCornerRadius}
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
      {shadowFilterId ? <PreviewSvgShadowOnlyFilter id={shadowFilterId} shadow={shadow} opacity={opacity} /> : null}
      {borderThickness > 0 || (fillIsFlat && fillCornerRadius > 0) ? (
        <defs>
          {borderThickness > 0 ? (
            <mask id={maskId}>
              <rect x={0} y={0} width={width} height={height} rx={cornerRadius} ry={cornerRadius} fill="white" />
              <rect
                x={innerTrackRect.x}
                y={innerTrackRect.y}
                width={innerTrackRect.width}
                height={innerTrackRect.height}
                rx={fillCornerRadius}
                ry={fillCornerRadius}
                fill="black"
              />
            </mask>
          ) : null}
          {fillIsFlat && fillCornerRadius > 0 ? (
            <clipPath id={flatFillClipId}>
              <rect x={layout.fillRect.x} y={layout.fillRect.y} width={layout.fillRect.width} height={layout.fillRect.height} />
            </clipPath>
          ) : null}
        </defs>
      ) : null}
      {borderThickness > 0 ? (
        <rect
          x={0}
          y={0}
          width={width}
          height={height}
          rx={cornerRadius}
          ry={cornerRadius}
          fill={data.track_border_color}
          mask={`url(#${maskId})`}
          opacity={opacity}
          filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}
        />
      ) : (
        <rect
          x={0}
          y={0}
          width={width}
          height={height}
          rx={cornerRadius}
          ry={cornerRadius}
          fill={data.track_empty_color}
          fillOpacity={data.track_empty_opacity}
          opacity={opacity}
          filter={shadowFilterId ? `url(#${shadowFilterId})` : undefined}
        />
      )}
      {borderThickness > 0 ? (
        <rect
          x={innerTrackRect.x}
          y={innerTrackRect.y}
          width={innerTrackRect.width}
          height={innerTrackRect.height}
          rx={fillCornerRadius}
          ry={fillCornerRadius}
          fill={data.track_empty_color}
          fillOpacity={data.track_empty_opacity}
          opacity={opacity}
        />
      ) : null}
      {filledTrack}
      {showLabels ? (
        <>
          <text x="4" y={height / 2 + labelFontSize * 0.35} fill={data.min_max_label_color} fontSize={labelFontSize} fontFamily={labelFontFamily}>
            {formatLinearGaugeLabel(layout.min)}
          </text>
          <text
            x={width - 4}
            y={height / 2 + labelFontSize * 0.35}
            fill={data.min_max_label_color}
            fontSize={labelFontSize}
            fontFamily={labelFontFamily}
            textAnchor="end"
          >
            {formatLinearGaugeLabel(layout.max)}
          </text>
        </>
      ) : null}
    </svg>
  )
}
