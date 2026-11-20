/* oxlint-disable react/only-export-components */

import { Image, Navigation, Presentation, Timer, Type } from 'lucide-react'
import {
  DISPLAY_TYPE_DEFINITIONS,
  WIDGET_CATEGORY_NAME_KEYS,
  WIDGET_DRAWER_GROUPS,
  WIDGET_TYPE_DEFINITIONS,
  getDisplayTypeLabelKey,
} from './widget-catalog'

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

const lucideIcons = { Image, Navigation, Presentation, Timer, Type }

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

/** Each option's selection is a complete creation request, so one entry can offer several widget types. */
function createMenuOptions(type) {
  const displayTypes = WIDGET_TYPE_DEFINITIONS[type].displayTypes
  if (displayTypes.length === 0) return [{ selection: { type } }]

  const options = []
  for (const value of displayTypes) {
    const definition = DISPLAY_TYPE_DEFINITIONS[value]
    if (definition.modes.length > 0) {
      for (const mode of definition.modes) {
        const icon = materializeIcon(mode.icon)
        options.push({ value: mode.value, labelKey: mode.labelKey, icon, selection: { type, [definition.modeSelectionField]: mode.value } })
      }
    } else {
      const icon = materializeIcon(definition.icon)
      options.push({ value, labelKey: getDisplayTypeLabelKey(type, value), icon, selection: { type, displayType: value } })
    }
  }
  return options
}

export const WIDGET_ICONS = {}
for (const [type, definition] of Object.entries(WIDGET_TYPE_DEFINITIONS)) {
  WIDGET_ICONS[type] = materializeIcon(definition.icon)
}

const groups = {}

for (const [category, nameKey] of Object.entries(WIDGET_CATEGORY_NAME_KEYS)) {
  groups[category] = { category, nameKey, items: [] }
}

/** One entry for a drawer group, which offers each member's own presets. */
function createGroupItem(group) {
  const options = group.options.map((option) => ({
    value: option.value,
    labelKey: option.labelKey,
    icon: materializeIcon(option.icon),
    selection: option.preset === undefined ? { type: option.type } : { type: option.type, preset: option.preset },
  }))
  return {
    key: group.name,
    types: [...new Set(group.options.map((option) => option.type))],
    icon: materializeIcon(group.icon),
    shortNameKey: group.shortNameKey,
    category: group.category,
    options,
  }
}

// A group takes the place of its first member, so the drawer keeps the manifest's order.
const placedGroups = new Set()
for (const [type, definition] of Object.entries(WIDGET_TYPE_DEFINITIONS)) {
  if (!definition.current) continue
  if (definition.drawerGroup !== null) {
    if (placedGroups.has(definition.drawerGroup)) continue
    placedGroups.add(definition.drawerGroup)
    const group = WIDGET_DRAWER_GROUPS[definition.drawerGroup]
    groups[group.category].items.push(createGroupItem(group))
    continue
  }
  groups[definition.category].items.push({
    key: type,
    types: [type],
    icon: WIDGET_ICONS[type],
    shortNameKey: definition.shortNameKey,
    category: definition.category,
    options: createMenuOptions(type),
  })
}

export const GROUPED_QUICKMENU_ITEMS = []
for (const group of Object.values(groups)) {
  if (group.items.length > 0) GROUPED_QUICKMENU_ITEMS.push(group)
}
