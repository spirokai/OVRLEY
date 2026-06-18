/**
 * Builds the shared preview model for intrinsic metric-style widgets.
 *
 * Computes the formatted value text, unit text, icon layout, and visual bounds
 * for a metric widget (speed, heartrate, cadence, power, time, temperature) at
 * the given preview time.
 *
 * Boxed display types (heading_tape, linear, bars, arc, corner) are skipped —
 * they use their own presentation-specific preview path driven by display_type.
 *
 * @param {object} params
 * @param {object} params.widget - Widget configuration object.
 * @param {object} params.activity - Activity data with series values.
 * @param {number} params.previewSecond - Current preview time in seconds.
 * @returns {object|null} Preview model with metricLayout, visualBounds, and text values, or null for non-value or boxed widgets.
 */

import { formatStandardMetricDisplay, formatTimeValue } from './formatUtils'
import { getMetricWidgetLayout, getMetricWidgetVisualBounds, getPreviewFontFamily } from './textMeasurement'
import { getInterpolatedActivityValue, getInterpolatedTimeValue } from '@/features/overlay-editor'
import { getStandardMetricDefinition, isStandardMetricWidgetType, isBoxedDisplayType } from '@/lib/widget/standard-metrics'
import { resolveActiveMetricWidgetData } from '@/lib/widget/metric-widget-resolver'

export function buildMetricWidgetPreviewModel({ widget, activity, previewSecond }) {
  // Guard — skip non-value widgets and gradient type (handled separately).
  if (!widget || widget.type === 'gradient') {
    return null
  }
  // Boxed display types use their own presentation-specific preview path.
  if (isBoxedDisplayType(widget?.data?.display_type)) {
    return null
  }
  if (widget.category !== 'values' && !isStandardMetricWidgetType(widget.type)) {
    return null
  }

  // Resolve display_variants for non-text display types
  const resolvedData = resolveActiveMetricWidgetData(widget.data)
  const fontSize = resolvedData.font_size ?? 60
  const fontFamily = getPreviewFontFamily(resolvedData.font || resolvedData.font_family)

  // Value formatting — format the interpolated activity value based on widget type (speed, heartrate, cadence, power, time, temperature)
  let valueText = '--'
  let unitText = ''

  if (isStandardMetricWidgetType(widget.type)) {
    const definition = getStandardMetricDefinition(widget.type)
    const formatted = formatStandardMetricDisplay(widget.type, getInterpolatedActivityValue(activity, widget.type, previewSecond), resolvedData)
    valueText = formatted.value
    unitText = formatted.units

    const showUnits = resolvedData.show_units ?? definition?.showUnitsByDefault ?? false
    const showIcon = resolvedData.show_icon ?? true
    const iconSize = resolvedData.icon_size ?? 28
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
        iconOffsetX: resolvedData.icon_offset_x ?? 0,
        iconOffsetY: resolvedData.icon_offset_y ?? 0,
      }),
    }
  } else if (widget.type === 'time') {
    valueText = formatTimeValue(resolvedData.format || 'time-24', getInterpolatedTimeValue(activity, previewSecond))
  } else {
    return null
  }

  // Layout computation — build icon, value, and units positions via text measurement, then compute visual bounds with icon offsets
  const showUnits = resolvedData.show_units ?? ['speed', 'temperature'].includes(widget.type)
  const showIcon = resolvedData.show_icon ?? true
  const iconSize = resolvedData.icon_size ?? 28
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
      iconOffsetX: resolvedData.icon_offset_x ?? 0,
      iconOffsetY: resolvedData.icon_offset_y ?? 0,
    }),
  }
}
