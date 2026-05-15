/* eslint-disable react-refresh/only-export-components */

import { Type } from 'lucide-react'
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

const widgetTypes = ['cadence', 'heartrate', 'power', 'speed', 'temperature', 'time', 'gradient', 'course', 'elevation']

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
  speed: widgetIconComponents.speed,
  elevation: widgetIconComponents.elevation,
  heartrate: widgetIconComponents.heartrate,
  power: widgetIconComponents.power,
  cadence: widgetIconComponents.cadence,
  time: widgetIconComponents.time,
  temperature: widgetIconComponents.temperature,
  gradient: widgetIconComponents.gradient,
  course: widgetIconComponents.course,
}

export const QUICKMENU_ITEMS = [
  { type: 'label', icon: Type, label: 'Text' },
  { type: 'speed', icon: widgetIconComponents.speed, label: 'Speed' },
  { type: 'elevation', icon: widgetIconComponents.elevation, label: 'Elev.' },
  { type: 'heartrate', icon: widgetIconComponents.heartrate, label: 'HR' },
  { type: 'power', icon: widgetIconComponents.power, label: 'Power' },
  { type: 'cadence', icon: widgetIconComponents.cadence, label: 'Cadence' },
  { type: 'time', icon: widgetIconComponents.time, label: 'Time' },
  { type: 'temperature', icon: widgetIconComponents.temperature, label: 'Temp.' },
  { type: 'gradient', icon: widgetIconComponents.gradient, label: 'Grad.' },
  { type: 'course', icon: widgetIconComponents.course, label: 'Map' },
]
