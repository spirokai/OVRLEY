/**
 * Builds the shared preview model for metric-style widgets.
 *
 * Computes the formatted value text, unit text, icon layout, and visual bounds
 * for a metric widget (speed, heartrate, cadence, power, time, temperature) at
 * the given preview time.
 *
 * @param {object} params
 * @param {object} params.widget - Widget configuration object.
 * @param {object} params.activity - Activity data with series values.
 * @param {number} params.previewSecond - Current preview time in seconds.
 * @returns {object|null} Preview model with metricLayout, visualBounds, and text values, or null for non-value widgets.
 */

import { formatSpeed, formatTemperature, formatTimeValue } from './formatUtils'
import { getMetricWidgetLayout, getMetricWidgetVisualBounds, getPreviewFontFamily } from './textMeasurement'
import { getInterpolatedActivityValue, getInterpolatedTimeValue } from '@/features/overlay-editor'

export function buildMetricWidgetPreviewModel({ widget, activity, previewSecond }) {
  // Guard — skip non-value widgets and gradient type (handled separately)
  if (!widget || widget.category !== 'values' || widget.type === 'gradient') {
    return null
  }

  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)

  // Value formatting — format the interpolated activity value based on widget type (speed, heartrate, cadence, power, time, temperature)
  let valueText = '--'
  let unitText = ''

  if (widget.type === 'speed') {
    const speedUnit = widget.data.speed_unit || (widget.data.unit === 'imperial' ? 'mph' : 'kmh')
    const formatted = formatSpeed(getInterpolatedActivityValue(activity, 'speed', previewSecond), speedUnit)
    valueText = formatted.value
    unitText = formatted.units
  } else if (widget.type === 'heartrate') {
    const value = getInterpolatedActivityValue(activity, 'heartrate', previewSecond)
    valueText = value === null || value === undefined ? '--' : Math.round(value).toString()
    unitText = 'BPM'
  } else if (widget.type === 'cadence') {
    const value = getInterpolatedActivityValue(activity, 'cadence', previewSecond)
    valueText = value === null || value === undefined ? '--' : Math.round(value).toString()
    unitText = 'RPM'
  } else if (widget.type === 'power') {
    const value = getInterpolatedActivityValue(activity, 'power', previewSecond)
    valueText = value === null || value === undefined ? '--' : Math.round(value).toString()
    unitText = 'W'
  } else if (widget.type === 'time') {
    valueText = formatTimeValue(widget.data.format || 'time-24', getInterpolatedTimeValue(activity, previewSecond))
  } else if (widget.type === 'temperature') {
    const formatted = formatTemperature(
      getInterpolatedActivityValue(activity, 'temperature', previewSecond),
      widget.data.temperature_unit || 'celsius',
    )
    valueText = formatted.value
    unitText = formatted.units
  } else {
    return null
  }

  // Layout computation — build icon, value, and units positions via text measurement, then compute visual bounds with icon offsets
  const showUnits = widget.data.show_units ?? ['speed', 'temperature'].includes(widget.type)
  const showIcon = widget.data.show_icon ?? true
  const iconSize = widget.data.icon_size ?? 28
  const metricLayout = getMetricWidgetLayout({
    fontSize,
    fontFamily,
    valueText,
    unitText,
    showIcon,
    showUnits,
    iconSize,
  })

  return {
    fontFamily,
    fontSize,
    iconSize,
    metricLayout,
    showIcon,
    showUnits,
    unitText,
    valueText,
    visualBounds: getMetricWidgetVisualBounds(metricLayout, {
      iconOffsetX: widget.data.icon_offset_x ?? 0,
      iconOffsetY: widget.data.icon_offset_y ?? 0,
    }),
  }
}
