/* eslint-disable react-refresh/only-export-components */

import { Presentation, Timer, Type } from 'lucide-react'
import {
  CURRENT_STANDARD_METRIC_WIDGET_TYPES,
  BACKDROP_TYPE_DEFINITIONS,
  BACKDROP_TYPE_LABEL_KEYS,
  DISPLAY_TYPE_LABEL_KEYS,
  LAP_TIMER_MODES,
  WIDGET_CATEGORY_NAME_KEYS,
  WIDGET_TYPE_DEFINITIONS,
  requireWidgetTypeDefinition,
} from './standard-widgets'
import { getSupportedDisplayTypes, isStandardMetricWidgetType } from './standard-metrics'
import { METRIC_ICON_SVGS, DISPLAY_TYPE_ICON_SVGS, getIconSvgByAssetFile } from './widget-icon-data'

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

function createLapTimerModeIcon({ source, name, assetFile }) {
  if (source === 'lucide') {
    if (name !== 'Timer') throw new Error(`Unsupported lap timer Lucide icon: ${name}`)
    return Timer
  }

  if (source !== 'shared' && source !== 'custom') throw new Error(`Unsupported lap timer icon source: ${source}`)

  const data = getIconSvgByAssetFile(assetFile)
  const Icon = (props) => <ParsedSvgIcon data={data} {...props} />
  Icon.displayName = `LapTimerModeIcon.${assetFile}`
  return Icon
}

/**
 * Returns the canonical UI label for an available activity attribute.
 * Attributes without a widget definition remain visible with a human-readable
 * form of their backend identifier.
 *
 * @param {string} type - Canonical activity attribute identifier.
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {string} Activity attribute label.
 */
export function getActivityAttributeLabel(type, translate) {
  const definition = WIDGET_TYPE_DEFINITIONS[type]
  if (definition) return translate(definition.nameKey)

  const words = []
  for (const part of type.split('_')) words.push(part === 'gps' ? 'GPS' : part.charAt(0).toUpperCase() + part.slice(1))
  return translate(`widgets.activityAttributes.${type}`, words.join(' '))
}

/**
 * Returns the translated long name for a configured widget type.
 * @param {string} type - Canonical widget type.
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {string} Translated widget name.
 */
export function getWidgetTypeName(type, translate) {
  const definition = requireWidgetTypeDefinition(type)
  return translate(definition.nameKey)
}

const widgetTypes = Object.keys(WIDGET_TYPE_DEFINITIONS).filter((type) => !['backdrop', 'label'].includes(type))

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
    shortNameKey: requireWidgetTypeDefinition(type).shortNameKey,
    category: requireWidgetTypeDefinition(type).category,
    options: getWidgetDisplayTypes(type).map((value) => ({
      value,
      labelKey: type === 'backdrop' ? BACKDROP_TYPE_LABEL_KEYS[value] : DISPLAY_TYPE_LABEL_KEYS[value],
      icon: DISPLAY_TYPE_ICONS[value],
      selection: { displayType: value },
    })),
  }))
  .concat({
    type: 'lap_timer',
    icon: Timer,
    shortNameKey: requireWidgetTypeDefinition('lap_timer').shortNameKey,
    category: requireWidgetTypeDefinition('lap_timer').category,
    options: LAP_TIMER_MODES.map((mode) => ({ ...mode, icon: createLapTimerModeIcon(mode.icon), selection: { lapTimerMode: mode.value } })),
  })

export const GROUPED_QUICKMENU_ITEMS = (() => {
  const groups = {}
  for (const item of QUICKMENU_ITEMS) {
    if (!groups[item.category]) groups[item.category] = []
    groups[item.category].push(item)
  }
  return Object.entries(WIDGET_CATEGORY_NAME_KEYS)
    .map(([category, nameKey]) => ({
      category,
      nameKey,
      items: groups[category] ?? [],
    }))
    .filter((group) => group.items.length > 0)
})()
