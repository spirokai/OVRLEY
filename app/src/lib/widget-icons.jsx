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

export const QUICKMENU_ITEMS = ['label', 'speed', 'elevation', 'heartrate', 'power', 'cadence', 'time', 'temperature', 'gradient', 'course'].map(
  (type) => ({
    type,
    icon: TYPE_ICONS[type],
    label: WIDGET_DRAWER_LABELS[type] ?? TYPE_LABELS[type],
  }),
)
