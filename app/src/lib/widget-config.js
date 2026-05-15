/**
 * Provides shared widget config utilities for the app.
 */

import { normalizeColorFields } from './color-utils'

const WIDGET_ID_PATTERN = /^(label|value|plot)-(\d+)$/

/**
 * Resolves widget target details directly from a widget id.
 *
 * @param {*} widgetId - Identifier of the target widget.
 * @returns {object|null} Derived data structure for downstream use.
 */
function resolveWidgetTarget(widgetId) {
  const match = WIDGET_ID_PATTERN.exec(String(widgetId || ''))
  if (!match) {
    return null
  }

  const [, prefix, rawIndex] = match
  const index = Number.parseInt(rawIndex, 10)
  if (!Number.isInteger(index) || index < 0) {
    return null
  }

  const category = prefix === 'label' ? 'labels' : prefix === 'value' ? 'values' : prefix === 'plot' ? 'plots' : null

  if (!category) {
    return null
  }

  return {
    category,
    id: `${prefix}-${index}`,
    index,
  }
}

/**
 * Updates a single widget entry immutably while preserving untouched branches.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @param {*} updater - Value for updater.
 * @returns {*} Result produced by the helper.
 */
function updateWidgetEntry(config, widgetId, updater) {
  if (!config || typeof updater !== 'function') {
    return config
  }

  const target = resolveWidgetTarget(widgetId)
  if (!target) {
    return config
  }

  const currentCollection = config[target.category]
  if (!Array.isArray(currentCollection)) {
    return config
  }

  const currentWidget = currentCollection[target.index]
  if (!currentWidget) {
    return config
  }

  const nextWidget = updater(currentWidget, target)
  if (!nextWidget || nextWidget === currentWidget) {
    return config
  }

  const nextCollection = [...currentCollection]
  nextCollection[target.index] = nextWidget

  return {
    ...config,
    [target.category]: nextCollection,
  }
}

/**
 * Builds config widgets.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {*} Derived data structure for downstream use.
 */
export function buildConfigWidgets(config) {
  if (!config) return []

  const widgets = []

  ;(config.labels || []).forEach((item, index) => {
    widgets.push({
      id: `label-${index}`,
      type: 'label',
      category: 'labels',
      index,
      name: item.text || 'Text',
      data: item,
    })
  })
  ;(config.values || []).forEach((item, index) => {
    widgets.push({
      id: `value-${index}`,
      type: item.value,
      category: 'values',
      index,
      name: item.value,
      data: item,
    })
  })
  ;(config.plots || []).forEach((item, index) => {
    widgets.push({
      id: `plot-${index}`,
      type: item.value,
      category: 'plots',
      index,
      name: item.value,
      data: item,
    })
  })

  return widgets
}

/**
 * Handles group widgets for sidebar.
 *
 * @param {*} widgets - Widget collection in the current template.
 * @param {*} typeLabels - Value for type labels.
 * @returns {*} Result produced by the helper.
 */
export function groupWidgetsForSidebar(widgets, typeLabels) {
  const grouped = widgets.reduce((accumulator, widget) => {
    const typeName = typeLabels[widget.type] || widget.type
    if (!accumulator[typeName]) accumulator[typeName] = []
    accumulator[typeName].push({
      ...widget,
      name: widget.type === 'label' ? widget.name : typeName,
    })
    return accumulator
  }, {})

  return Object.keys(grouped)
    .sort()
    .flatMap((typeName) =>
      grouped[typeName].map((widget, widgetIndex) => ({
        ...widget,
        groupLabel: widgetIndex === 0 ? typeName : null,
      })),
    )
}

/**
 * Finds widget by id.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @returns {*} Requested value or structure.
 */
/**
 * Updates widget in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @param {*} updates - Partial changes to merge into the target.
 * @returns {*} Result produced by the helper.
 */
export function updateWidgetInConfig(config, widgetId, updates) {
  const normalizedUpdates = normalizeColorFields(updates)
  return updateWidgetEntry(config, widgetId, (currentWidget) => ({
    ...currentWidget,
    ...normalizedUpdates,
  }))
}

/**
 * Updates widgets in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} updatesById - Value for updates by id.
 * @returns {*} Result produced by the helper.
 */
export function updateWidgetsInConfig(config, updatesById) {
  if (!config || !updatesById || typeof updatesById !== 'object') {
    return config
  }

  const widgetUpdates = Object.entries(updatesById).filter(([, updates]) => updates && typeof updates === 'object')
  if (!widgetUpdates.length) {
    return config
  }

  const nextCollectionsByCategory = new Map()
  let hasChanges = false

  widgetUpdates.forEach(([widgetId, updates]) => {
    const target = resolveWidgetTarget(widgetId)
    if (!target) {
      return
    }

    const sourceCollection = nextCollectionsByCategory.has(target.category) ? nextCollectionsByCategory.get(target.category) : config[target.category]
    const currentWidget = Array.isArray(sourceCollection) ? sourceCollection[target.index] : null

    if (!currentWidget) {
      return
    }

    const nextWidget = {
      ...currentWidget,
      ...normalizeColorFields(updates),
    }
    const nextCollection = [...sourceCollection]
    nextCollection[target.index] = nextWidget
    nextCollectionsByCategory.set(target.category, nextCollection)
    hasChanges = true
  })

  if (!hasChanges) {
    return config
  }

  return {
    ...config,
    ...Object.fromEntries(nextCollectionsByCategory),
  }
}

/**
 * Replaces widget in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @param {*} nextWidgetData - Value for next widget data.
 * @returns {*} Result produced by the helper.
 */
export function replaceWidgetInConfig(config, widgetId, nextWidgetData) {
  return updateWidgetEntry(config, widgetId, () => nextWidgetData)
}

/**
 * Deletes widget in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @returns {*} Result produced by the helper.
 */
export function deleteWidgetInConfig(config, widgetId) {
  if (!config) return config

  const target = resolveWidgetTarget(widgetId)
  if (!target) {
    return config
  }

  const currentCollection = config[target.category]
  if (!Array.isArray(currentCollection) || !currentCollection[target.index]) {
    return config
  }

  return {
    ...config,
    [target.category]: currentCollection.filter((_, index) => index !== target.index),
  }
}

/**
 * Deletes widgets in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetIds - Value for widget ids.
 * @returns {*} Result produced by the helper.
 */
export function deleteWidgetsInConfig(config, widgetIds) {
  if (!config || !Array.isArray(widgetIds) || !widgetIds.length) {
    return config
  }

  const indexesByCategory = widgetIds.reduce((accumulator, widgetId) => {
    const target = resolveWidgetTarget(widgetId)
    if (!target || !config?.[target.category]?.[target.index]) {
      return accumulator
    }

    if (!accumulator[target.category]) {
      accumulator[target.category] = new Set()
    }

    accumulator[target.category].add(target.index)
    return accumulator
  }, {})

  const categories = Object.keys(indexesByCategory)
  if (!categories.length) {
    return config
  }

  return categories.reduce(
    (nextConfig, category) => ({
      ...nextConfig,
      [category]: (nextConfig[category] || []).filter((_, index) => !indexesByCategory[category].has(index)),
    }),
    config,
  )
}
