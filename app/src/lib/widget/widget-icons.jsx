/* eslint-disable react-refresh/only-export-components */

import { Presentation, Timer, Type } from 'lucide-react'
import {
  CURRENT_STANDARD_METRIC_WIDGET_TYPES,
  STANDARD_METRIC_WIDGET_TYPES,
  BACKDROP_TYPE_DEFINITIONS,
  BACKDROP_TYPE_LABELS,
  DISPLAY_TYPE_LABELS,
  LAP_TIMER_MODES,
} from './standard-widgets'
import { getStandardMetricDefinition, getSupportedDisplayTypes, isStandardMetricWidgetType } from './standard-metrics'
import { METRIC_ICON_SVGS, DISPLAY_TYPE_ICON_SVGS } from './widget-icon-data'

export { METRIC_ICON_SVGS, DISPLAY_TYPE_ICON_SVGS }

function ParsedSvgIcon({ data, className, ...props }) {
  if (!data?.innerMarkup) return null
  return (
    <svg
      viewBox="0 0 24 24"
      color="currentColor"
      fill={data.fill}
      stroke={data.stroke}
      strokeWidth={data.strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      {...props}
      dangerouslySetInnerHTML={{ __html: data.innerMarkup }}
    />
  )
}

export function WidgetIcon({ type, ...props }) {
  return <ParsedSvgIcon data={METRIC_ICON_SVGS[type]} {...props} />
}

export function DisplayTypeIcon({ displayType, ...props }) {
  return <ParsedSvgIcon data={DISPLAY_TYPE_ICON_SVGS[displayType]} {...props} />
}

const STANDARD_METRIC_TYPE_LABELS = Object.fromEntries(
  STANDARD_METRIC_WIDGET_TYPES.map((type) => [type, getStandardMetricDefinition(type)?.label || type]),
)

// General widget labels used throughout the app.
export const TYPE_LABELS = {
  backdrop: 'Backdrop',
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
  backdrop: 'Backdrop',
  label: 'Text',
  elevation: 'Elev.',
  heartrate: 'HR',
  time: 'Time',
  temperature: 'Temp.',
  gradient: 'Grad.',
  course: 'Map',
  air_pressure: 'Air Press.',
  distance: 'Dist.',
  ground_contact_time: 'GCT',
  left_right_balance: 'L/R Bal.',
  stride_length: 'Stride',
  stroke_rate: 'S/R',
  vertical_speed: 'V. Speed',
  gear_position: 'Gear',
  vertical_oscillation: 'V. Osc.',
  core_temperature: 'Core T.',
  throttle_position: 'Throttle',
  brake_position: 'Brake',
  lean_angle: 'Lean',
}

const widgetTypes = Object.keys(TYPE_LABELS).filter((type) => !['backdrop', 'label'].includes(type))

const widgetIconComponents = {}
widgetTypes.forEach((type) => {
  const C = (props) => <WidgetIcon type={type} {...props} />
  C.displayName = `WidgetIcon.${type}`
  widgetIconComponents[type] = C
})

export const WIDGET_ICONS = {
  backdrop: Presentation,
  label: Type,
  ...widgetIconComponents,
}

export const TYPE_ICONS = {
  backdrop: Presentation,
  label: Type,
  ...widgetIconComponents,
  lap_timer: Timer,
}

export const DISPLAY_TYPE_ICONS = Object.fromEntries(
  Object.keys(DISPLAY_TYPE_ICON_SVGS).map((dt) => [dt, (props) => <DisplayTypeIcon displayType={dt} {...props} />]),
)

const BACKDROP_DISPLAY_TYPES = Object.keys(BACKDROP_TYPE_DEFINITIONS)

export function getWidgetDisplayTypes(type) {
  if (type === 'backdrop') return BACKDROP_DISPLAY_TYPES
  if (isStandardMetricWidgetType(type)) return getSupportedDisplayTypes(type)
  return ['text']
}

export const QUICKMENU_ITEMS = ['label', 'time', 'elevation', 'course', 'gradient', 'backdrop', ...CURRENT_STANDARD_METRIC_WIDGET_TYPES]
  .filter((type) => type !== 'lap_timer')
  .map((type) => ({
    type,
    icon: TYPE_ICONS[type],
    label: WIDGET_DRAWER_LABELS[type] ?? TYPE_LABELS[type],
    options: getWidgetDisplayTypes(type).map((value) => ({
      value,
      label: type === 'backdrop' ? (BACKDROP_TYPE_LABELS[value] ?? value) : (DISPLAY_TYPE_LABELS[value] ?? value),
      icon: DISPLAY_TYPE_ICONS[value],
      selection: { displayType: value },
    })),
  }))
  .concat({
    type: 'lap_timer',
    icon: Timer,
    label: 'Lap Timer',
    options: LAP_TIMER_MODES.map((mode) => ({ ...mode, icon: Timer, selection: { lapTimerMode: mode.value } })),
  })

const NON_METRIC_CATEGORIES = {
  backdrop: 'general',
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

const CATEGORY_ORDER = ['general', 'cycling', 'running', 'motosports', 'camera', 'other']

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
