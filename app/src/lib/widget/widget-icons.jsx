/* eslint-disable react-refresh/only-export-components */

import { Image, Presentation, Timer, Type } from 'lucide-react'
import { BACKDROP_TYPE_DEFINITIONS, DISPLAY_TYPE_DEFINITIONS, WIDGET_CATEGORY_NAME_KEYS, WIDGET_TYPE_DEFINITIONS } from './standard-widgets'
import { getSupportedDisplayTypes, isStandardMetricWidgetType } from './standard-metrics'

const iconAssetModules = import.meta.glob('../../../../assets/widget-icons/*.svg', {
  eager: true,
  import: 'default',
  query: '?raw',
})

const iconSvgMarkupByAssetFile = {}
for (const [path, svgMarkup] of Object.entries(iconAssetModules)) {
  const assetFile = path.slice(path.lastIndexOf('/') + 1)
  iconSvgMarkupByAssetFile[assetFile] = svgMarkup
}
const parsedIconAssets = new Map()

const lucideIcons = { Image, Presentation, Timer, Type }

function parseIconSvg(svgMarkup) {
  const rootTag = svgMarkup.match(/<svg[^>]*>/i)?.[0]
  if (!rootTag) throw new Error('Metric icon asset must contain an SVG root element')

  const strokeWidthMatch = svgMarkup.match(/stroke-width="([^"]+)"/)
  const innerMarkupMatch = svgMarkup.match(/<svg[^>]*>([\s\S]*?)<\/svg>/i)
  const fill = rootTag.match(/\sfill="([^"]+)"/)?.[1]
  const stroke = rootTag.match(/\sstroke="([^"]+)"/)?.[1]
  if (!fill || !stroke) throw new Error('Metric icon SVG root must define fill and stroke')

  return {
    fill,
    stroke,
    strokeWidth: Number(strokeWidthMatch?.[1] || 2),
    innerMarkup: (innerMarkupMatch?.[1] || '').trim(),
  }
}

function getIconSvgByAssetFile(assetFile) {
  if (!Object.hasOwn(iconSvgMarkupByAssetFile, assetFile)) throw new Error(`Unsupported icon asset: ${assetFile}`)
  if (!parsedIconAssets.has(assetFile)) {
    const data = parseIconSvg(iconSvgMarkupByAssetFile[assetFile])
    parsedIconAssets.set(assetFile, data)
  }
  return parsedIconAssets.get(assetFile)
}

function ParsedSvgIcon(data, props) {
  return (
    <svg
      viewBox="0 0 24 24"
      color="currentColor"
      fill={data.fill}
      stroke={data.stroke}
      strokeWidth={data.strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
      dangerouslySetInnerHTML={{ __html: data.innerMarkup }}
    />
  )
}

function materializeIcon(icon) {
  if (icon.assetFile !== undefined) {
    const data = getIconSvgByAssetFile(icon.assetFile)
    return ParsedSvgIcon.bind(null, data)
  }
  const Icon = lucideIcons[icon.name]
  if (icon.source !== 'lucide' || !Icon) throw new Error('Unsupported manifest icon: ' + icon.name)
  return Icon
}

function createMenuOptions(type) {
  let definitions
  let displayTypes
  if (type === 'backdrop') {
    definitions = BACKDROP_TYPE_DEFINITIONS
    displayTypes = Object.keys(definitions)
  } else if (isStandardMetricWidgetType(type)) {
    definitions = DISPLAY_TYPE_DEFINITIONS
    displayTypes = getSupportedDisplayTypes(type)
  } else {
    return [{ selection: {} }]
  }

  const options = []
  for (const value of displayTypes) {
    const definition = definitions[value]
    if (definition.modes !== undefined) {
      for (const mode of definition.modes) {
        const icon = materializeIcon(mode.icon)
        options.push({ value: mode.value, labelKey: mode.labelKey, icon, selection: { lapTimerMode: mode.value } })
      }
    } else {
      const icon = materializeIcon(definition.icon)
      options.push({ value, labelKey: definition.labelKey, icon, selection: { displayType: value } })
    }
  }
  return options
}

export const WIDGET_ICON_SVGS = {}
export const WIDGET_ICONS = {}
for (const [type, definition] of Object.entries(WIDGET_TYPE_DEFINITIONS)) {
  if (definition.icon.assetFile !== undefined) {
    WIDGET_ICON_SVGS[type] = getIconSvgByAssetFile(definition.icon.assetFile)
  }
  WIDGET_ICONS[type] = materializeIcon(definition.icon)
}

const groups = {}
for (const [category, nameKey] of Object.entries(WIDGET_CATEGORY_NAME_KEYS)) {
  groups[category] = { category, nameKey, items: [] }
}
for (const [type, definition] of Object.entries(WIDGET_TYPE_DEFINITIONS)) {
  if (isStandardMetricWidgetType(type) && !definition.current) continue
  const options = createMenuOptions(type)
  groups[definition.category].items.push({
    type,
    icon: WIDGET_ICONS[type],
    shortNameKey: definition.shortNameKey,
    category: definition.category,
    options,
  })
}

export const GROUPED_QUICKMENU_ITEMS = []
for (const group of Object.values(groups)) {
  if (group.items.length > 0) GROUPED_QUICKMENU_ITEMS.push(group)
}
