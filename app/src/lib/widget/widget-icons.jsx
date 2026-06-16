/* eslint-disable react-refresh/only-export-components */

import { Type } from 'lucide-react'
import { CURRENT_STANDARD_METRIC_WIDGET_TYPES, STANDARD_METRIC_WIDGET_TYPES } from './standard-widgets'
import { getStandardMetricDefinition } from './standard-metrics'
import { METRIC_ICON_SVGS } from './widget-icon-data'

export { METRIC_ICON_SVGS }

export function WidgetIcon({ type, className, ...props }) {
  const data = METRIC_ICON_SVGS[type]
  if (!data?.innerMarkup) return null
  return (
    <svg
      viewBox="0 0 24 24"
      color="currentColor"
      fill="none"
      stroke="currentColor"
      strokeWidth={data.strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      {...props}
      dangerouslySetInnerHTML={{ __html: data.innerMarkup }}
    />
  )
}

const STANDARD_METRIC_TYPE_LABELS = Object.fromEntries(
  STANDARD_METRIC_WIDGET_TYPES.map((type) => [type, getStandardMetricDefinition(type)?.label || type]),
)

// General widget labels used throughout the app.
export const TYPE_LABELS = {
  label: 'Text',
  course: 'Route Map',
  elevation: 'Elevation',
  gradient: 'Gradient',
  time: 'Time',
  heading: 'Heading',
  ...STANDARD_METRIC_TYPE_LABELS,
}

// Labels for the widget drawer, which may be shorter than the general labels

export const WIDGET_DRAWER_LABELS = {
  label: 'Text',
  elevation: 'Elev.',
  heartrate: 'HR',
  time: 'Time',
  temperature: 'Temp.',
  gradient: 'Grad.',
  course: 'Map',
  air_pressure: 'Air Press.',
  ground_contact_time: 'GCT',
  left_right_balance: 'L/R Bal.',
  stride_length: 'Stride',
  stroke_rate: 'S/R',
  vertical_speed: 'V. Speed',
  gear_position: 'Gear',
  vertical_oscillation: 'V. Osc.',
  core_temperature: 'Core T.',
}

const widgetTypes = Object.keys(TYPE_LABELS).filter((type) => type !== 'label')

const widgetIconComponents = {}
widgetTypes.forEach((type) => {
  const C = (props) => <WidgetIcon type={type} {...props} />
  C.displayName = `WidgetIcon.${type}`
  widgetIconComponents[type] = C
})

export const WIDGET_ICONS = {
  label: Type,
  ...widgetIconComponents,
}

export const TYPE_ICONS = {
  label: Type,
  ...widgetIconComponents,
}

export const QUICKMENU_ITEMS = ['label', 'time', 'elevation', 'course', 'gradient', ...CURRENT_STANDARD_METRIC_WIDGET_TYPES].map((type) => ({
  type,
  icon: TYPE_ICONS[type],
  label: WIDGET_DRAWER_LABELS[type] ?? TYPE_LABELS[type],
}))

const NON_METRIC_CATEGORIES = {
  label: 'general',
  time: 'general',
  elevation: 'general',
  course: 'general',
  gradient: 'general',
}

function getWidgetCategory(type) {
  if (type in NON_METRIC_CATEGORIES) return NON_METRIC_CATEGORIES[type]
  return getStandardMetricDefinition(type)?.category || 'other'
}

const CATEGORY_ORDER = ['general', 'cycling', 'running', 'camera', 'other']

export const GROUPED_QUICKMENU_ITEMS = (() => {
  const groups = {}
  for (const item of QUICKMENU_ITEMS) {
    const cat = getWidgetCategory(item.type)
    if (!groups[cat]) groups[cat] = []
    groups[cat].push(item)
  }
  return CATEGORY_ORDER.map((cat) => ({
    category: cat,
    items: groups[cat] || [],
  })).filter((g) => g.items.length > 0)
})()
