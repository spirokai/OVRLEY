/**
 * Provides overlay editor helpers for metric widget preview geometry.
 */

import {
  formatSpeed,
  formatTemperature,
  formatTimeValue,
  getMetricWidgetLayout,
  getMetricWidgetVisualBounds,
  getPreviewFontFamily,
} from './metricTextUtils'
import { getInterpolatedActivityValue, getInterpolatedTimeValue } from './utils'

/**
 * Builds the shared preview model for metric-style widgets.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.widget - Widget definition being rendered.
 * @param {*} options.activity - Parsed preview activity.
 * @param {*} options.previewSecond - Preview time in seconds.
 * @returns {object|null} Shared preview geometry for metric widgets.
 */
export function buildMetricWidgetPreviewModel({ widget, activity, previewSecond }) {
  if (!widget || widget.category !== 'values' || widget.type === 'gradient') {
    return null
  }

  const fontSize = widget.data.font_size ?? 60
  const fontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)

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
