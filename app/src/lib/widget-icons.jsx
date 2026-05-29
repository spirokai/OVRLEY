/* eslint-disable react-refresh/only-export-components */

import { Type } from 'lucide-react'
import { STANDARD_METRIC_WIDGET_TYPES, getStandardMetricDefinition } from './standard-metrics'
import { METRIC_ICON_SVGS } from './widget-icon-data'

export { METRIC_ICON_SVGS }

export function WidgetIcon({ type, className, ...props }) {
  const data = METRIC_ICON_SVGS[type]
  if (!data?.innerMarkup) return null
  return (
    <svg
      viewBox="0 0 24 24"
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
  speed: 'Speed',
  elevation: 'Elev.',
  heartrate: 'HR',
  power: 'Power',
  cadence: 'Cadence',
  time: 'Time',
  temperature: 'Temp.',
  gradient: 'Grad.',
  course: 'Map',
  pace: 'Pace',
  g_force: 'G-Force',
  air_pressure: 'Air Press.',
  ground_contact_time: 'GCT',
  left_right_balance: 'L/R Bal.',
  stride_length: 'Stride',
  stroke_rate: 'S/R',
  torque: 'Torque',
  vertical_speed: 'V. Speed',
  gear_position: 'Gear',
  vertical_oscillation: 'V. Osc.',
  core_temperature: 'Core T.',
  heading: 'Heading',
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

export const QUICKMENU_ITEMS = [
  'label',
  'speed',
  'elevation',
  'heartrate',
  'power',
  'cadence',
  'time',
  'temperature',
  'gradient',
  'course',
  'pace',
  'g_force',
  'air_pressure',
  'ground_contact_time',
  'left_right_balance',
  'stride_length',
  'stroke_rate',
  'torque',
  'vertical_speed',
  'gear_position',
  'vertical_oscillation',
  'core_temperature',
  'heading',
].map((type) => ({
  type,
  icon: TYPE_ICONS[type],
  label: WIDGET_DRAWER_LABELS[type] ?? TYPE_LABELS[type],
}))
