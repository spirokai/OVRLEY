/**
 * Builds the shared preview model for intrinsic metric-style widgets.
 *
 * Computes the formatted value text, unit text, icon layout, and visual bounds
 * for a metric widget (speed, heartrate, cadence, power, time, temperature) at
 * the given preview time.
 *
 * Boxed display types (heading_tape, linear, arc, corner) are skipped —
 * they use their own presentation-specific preview path driven by display_type.
 *
 * @param {object} params
 * @param {object} params.widget - Widget configuration object.
 * @param {object} params.activity - Activity data with series values.
 * @param {number} params.previewSecond - Current preview time in seconds.
 * @returns {object|null} Preview model with metricLayout, visualBounds, and text values, or null for non-value or boxed widgets.
 */

import { formatStandardMetricDisplay, formatTimeValue } from './format'
import {
  getMetricWidgetLayout,
  getMetricWidgetVisualBounds,
  getContentAlignmentOrigin,
  getPreviewFontFamily,
  getPreviewTextBaseline,
  getPreviewVerticalMetrics,
  measureArcPreviewText,
  measurePreviewText,
} from '../../shared/textMeasurement'
import {
  getPreviewActivity,
  getInterpolatedActivityValue,
  getInterpolatedTimeValue,
  getMetricSeries,
  resolveMetricPresentationValues,
} from '@/features/overlay-editor/utils/overlayEditorUtils'
import { NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT } from '@/features/overlay-editor/data/overlayEditorConstants'
import { interpolateNumericSeries } from '@/lib/interpolation'
import { isStandardMetricWidgetType, isBoxedDisplayType } from '@/lib/widget/standard-metrics'

const COORDINATE_DIRECTION_GAP_PX = 8

function getInterpolatedCoordinateValue(activity, componentIndex, previewSecond) {
  if (activity === null) return null

  const coordinateSeries = []
  for (const point of activity.course) coordinateSeries.push(point[componentIndex])
  return interpolateNumericSeries(activity.sample_elapsed_seconds, coordinateSeries, previewSecond)
}

/**
 * Returns the last finite numeric value in a metric series.
 * @param {unknown[]} series - Activity metric samples.
 * @returns {number|null} Last finite value, or null when none exists.
 */
function getLastFiniteValue(series) {
  for (let index = series.length - 1; index >= 0; index -= 1) {
    const candidate = series[index]
    if (candidate !== null && candidate !== undefined) return candidate
  }

  return null
}

/**
 * Formats current distance, optionally paired with the activity's total distance.
 * @param {object} activity - Activity data containing distance samples.
 * @param {number} previewSecond - Current preview time.
 * @param {object} widgetData - Normalized distance-widget data.
 * @returns {{value: string, units: string}} Formatted distance display.
 */
export function formatDistancePreviewDisplay(activity, previewSecond, widgetData) {
  const currentDistance = getInterpolatedActivityValue(activity, 'distance', previewSecond)
  const current = formatStandardMetricDisplay('distance', currentDistance, widgetData)
  if (!widgetData.show_full_distance || activity === null) return current

  const totalDistance = getLastFiniteValue(getMetricSeries(activity, 'distance') ?? [])
  if (totalDistance === null) return current

  const total = formatStandardMetricDisplay('distance', totalDistance, {
    ...widgetData,
    show_units: false,
  })

  return {
    value: `${current.value}/${total.value}`,
    units: current.units,
  }
}

/**
 * Formats current ascent, optionally paired with the full activity ascent.
 * @param {object} activity - Activity data containing total_ascent samples.
 * @param {number} previewSecond - Current preview time.
 * @param {object} widgetData - Normalized ascent-widget data.
 * @returns {{value: string, units: string}} Formatted ascent display.
 */
export function formatTotalAscentPreviewDisplay(activity, previewSecond, widgetData) {
  const currentAscent = getInterpolatedActivityValue(activity, 'total_ascent', previewSecond)
  const current = formatStandardMetricDisplay('total_ascent', currentAscent, widgetData)
  if (!widgetData.show_full_ascent || activity === null) return current

  const totalAscent = getLastFiniteValue(getMetricSeries(activity, 'total_ascent') ?? [])
  if (totalAscent === null) return current

  const total = formatStandardMetricDisplay('total_ascent', totalAscent, {
    ...widgetData,
    show_units: false,
  })
  return {
    value: `${current.value}/${total.value}`,
    units: current.units,
  }
}

function buildCoordinateLayout({ widget, formatted, fontFamily }) {
  const lineFontSize = formatted.lines.length === 2 ? widget.data.font_size * 0.4 : widget.data.font_size
  const lineHeight = lineFontSize * 0.92
  const lineGap = formatted.lines.length === 2 ? lineFontSize * 0.08 : 0
  const directionGap = Math.max(lineFontSize * 0.08, COORDINATE_DIRECTION_GAP_PX)
  const lines = []
  for (const line of formatted.lines) {
    const valueText = `${widget.data.prefix}${line.valueText}${widget.data.suffix}`
    const directionMeasure = measurePreviewText(line.direction, lineFontSize, fontFamily)
    const valueMeasure = measurePreviewText(valueText, lineFontSize, fontFamily)
    const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, lineFontSize, fontFamily)
    const baseline = getPreviewTextBaseline({
      lineHeight,
      ascent: valueVerticalMetrics.ascent,
      glyphHeight: valueVerticalMetrics.glyphHeight,
    })
    lines.push({
      ...line,
      valueText,
      directionWidth: directionMeasure.width,
      valueWidth: valueMeasure.width,
      baseline,
    })
  }
  let directionColumnWidth = 0
  let valueColumnWidth = 0
  for (const line of lines) {
    directionColumnWidth = Math.max(directionColumnWidth, line.directionWidth)
    valueColumnWidth = Math.max(valueColumnWidth, line.valueWidth)
  }
  const textWidth = valueColumnWidth + (directionColumnWidth ? directionColumnWidth + directionGap : 0)
  const totalTextHeight = lineHeight * lines.length + lineGap * Math.max(lines.length - 1, 0)
  const iconSize = widget.data.icon_size
  const rowHeight = Math.max(widget.data.show_icon ? iconSize : 0, totalTextHeight)
  const textGroupLeft = widget.data.show_icon ? iconSize + 8 + Math.max(widget.data.font_size * 0.08, 8) : 0
  const textTop = (rowHeight - totalTextHeight) / 2
  const width = textGroupLeft + textWidth
  const rowOriginX = getContentAlignmentOrigin(widget.data.content_alignment, 0, width)

  return {
    fontSize: lineFontSize,
    icon: widget.data.show_icon ? { left: rowOriginX, top: (rowHeight - iconSize) / 2, size: iconSize } : null,
    lines: buildPositionedCoordinateLines(
      lines,
      rowOriginX + textGroupLeft,
      textTop,
      lineHeight,
      lineGap,
      directionColumnWidth,
      valueColumnWidth,
      directionGap,
    ),
    width,
    height: rowHeight,
    rowOriginX,
  }
}

function buildPositionedCoordinateLines(lines, textGroupLeft, textTop, lineHeight, lineGap, directionColumnWidth, valueColumnWidth, directionGap) {
  const positionedLines = []
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]
    positionedLines.push({
      ...line,
      directionLeft: textGroupLeft,
      valueLeft: textGroupLeft + (directionColumnWidth ? directionColumnWidth + directionGap : 0) + (valueColumnWidth - line.valueWidth),
      baseline: textTop + index * (lineHeight + lineGap) + line.baseline,
    })
  }
  return positionedLines
}

function getCoordinateVisualBounds(layout, widgetData) {
  const minX = Math.min(layout.rowOriginX, layout.icon ? layout.icon.left + widgetData.icon_offset_x : layout.rowOriginX)
  const minY = Math.min(0, layout.icon ? layout.icon.top + widgetData.icon_offset_y : 0)
  const maxX = Math.max(
    layout.rowOriginX + layout.width,
    layout.icon ? layout.icon.left + widgetData.icon_offset_x + layout.icon.size : layout.rowOriginX,
  )
  const maxY = Math.max(layout.height, layout.icon ? layout.icon.top + widgetData.icon_offset_y + layout.icon.size : 0)
  return {
    minX,
    minY,
    maxX,
    maxY,
    width: maxX - minX,
    height: maxY - minY,
    offsetX: -minX,
    offsetY: -minY,
  }
}

const SPECIAL_METRIC_PREVIEW_FORMATTERS = {
  distance: formatDistancePreviewDisplay,
  total_ascent: formatTotalAscentPreviewDisplay,
}

function formatMetricWidgetValue({ widget, activity, previewSecond }) {
  const specialFormatter = SPECIAL_METRIC_PREVIEW_FORMATTERS[widget.type]
  if (specialFormatter) return specialFormatter(activity, previewSecond, widget.data)

  if (widget.type === 'gps_coordinates') {
    return formatStandardMetricDisplay(
      widget.type,
      [getInterpolatedCoordinateValue(activity, 0, previewSecond), getInterpolatedCoordinateValue(activity, 1, previewSecond)],
      widget.data,
    )
  }

  const { value } = resolveMetricPresentationValues(widget, activity, previewSecond)
  return formatStandardMetricDisplay(widget.type, value, widget.data)
}

/**
 * Builds the text content consumed by boxed gauges that retain the metric
 * value in their frame. Unlike the intrinsic metric model, this deliberately
 * has no icon layout: arc gauges stack value and unit vertically.
 *
 * @param {object} params
 * @param {object} params.widget - Resolved metric widget.
 * @param {number|null} params.presentationValue - Resolved value shown by the gauge.
 * @returns {{ valueText: string, unitText: string, fontFamily: string, fontSize: number, valueMeasure: object, valueVerticalMeasure: object, unitMeasure: object|null }|null}
 */
export function buildArcGaugeInnerWidgetModel({ widget, presentationValue }) {
  const formatted = formatStandardMetricDisplay(widget.type, presentationValue, widget.data)
  if (widget.type === 'gps_coordinates') {
    throw new Error('GPS coordinate widgets only support text display')
  }
  const fontFamily = getPreviewFontFamily(widget.data.font)
  const valueText = `${widget.data.prefix}${formatted.value}${widget.data.suffix}`
  const unitText = widget.data.show_units ? formatted.units : ''
  const valueMeasure = measureArcPreviewText(valueText, widget.data.font_size, fontFamily)
  const valueVerticalMeasure = measureArcPreviewText(
    valueText === 'N' || /^[0-9:.%+-]+$/.test(valueText) ? NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT : valueText,
    widget.data.font_size,
    fontFamily,
  )
  const unitMeasure = unitText ? measureArcPreviewText(unitText, Math.max(widget.data.font_size * 0.28, 12), fontFamily) : null

  return {
    valueText,
    unitText,
    fontFamily,
    valueMeasure,
    valueVerticalMeasure,
    unitMeasure,
  }
}

/**
 * Builds the formatted layout model for an intrinsic metric widget preview.
 * @param {object} params - Widget and current activity preview state.
 * @returns {object|null} Metric presentation model, or null for unsupported presentations.
 */
export function buildMetricWidgetPreviewModel({ widget, activity, previewSecond }) {
  // Guard — skip non-value widgets and gradient type (handled separately).
  if (widget.category !== 'values' || widget.type === 'gradient') return null
  // Boxed display types use their own presentation-specific preview path.
  if (isBoxedDisplayType(widget.data.display_type)) return null

  const displayActivity = getPreviewActivity(activity, previewSecond)

  // Resolve display_variants for non-text display types
  const fontFamily = getPreviewFontFamily(widget.data.font)

  // Value formatting — format the interpolated activity value based on widget type (speed, heartrate, cadence, power, time, temperature)
  let valueText
  let unitText

  if (isStandardMetricWidgetType(widget.type)) {
    const formatted = formatMetricWidgetValue({ widget, activity: displayActivity, previewSecond })
    if (widget.type === 'gps_coordinates') {
      const coordinateLayout = buildCoordinateLayout({ widget, formatted, fontFamily })
      return {
        content: {
          type: 'coordinates',
          layout: coordinateLayout,
        },
        metricLayout: coordinateLayout,
        visualBounds: getCoordinateVisualBounds(coordinateLayout, widget.data),
      }
    }
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'time') {
    valueText = formatTimeValue(widget.data.format, getInterpolatedTimeValue(displayActivity, previewSecond), displayActivity?.metadata?.timezone)
  } else {
    throw new Error(`Cannot build intrinsic metric preview for widget type: ${widget.type}`)
  }

  // Layout computation — build icon, value, and units positions via text measurement, then compute visual bounds with icon offsets
  const metricLayout = getMetricWidgetLayout({
    fontSize: widget.data.font_size,
    fontFamily,
    valueText,
    unitText,
    showIcon: widget.data.show_icon,
    showUnits: widget.data.show_units,
    iconSize: widget.data.icon_size,
    contentAlignment: widget.data.content_alignment,
  })

  return {
    content: {
      type: 'standard',
      valueText,
      unitText,
      layout: metricLayout,
    },
    metricLayout,
    unitText,
    valueText,
    visualBounds: getMetricWidgetVisualBounds(metricLayout, {
      iconOffsetX: widget.data.icon_offset_x,
      iconOffsetY: widget.data.icon_offset_y,
    }),
  }
}
