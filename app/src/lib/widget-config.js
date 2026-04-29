/**
 * Provides shared widget config utilities for the app.
 */

import { normalizeColorFields } from './color-utils'

/**
 * Handles clone config.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {*} Result produced by the helper.
 */
export function cloneConfig(config) {
  return JSON.parse(JSON.stringify(config))
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
export function findWidgetById(config, widgetId) {
  return (
    buildConfigWidgets(config).find((widget) => widget.id === widgetId) || null
  )
}

/**
 * Updates widget in config.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @param {*} updates - Partial changes to merge into the target.
 * @returns {*} Result produced by the helper.
 */
export function updateWidgetInConfig(config, widgetId, updates) {
  if (!config) return config

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  const currentWidget = nextConfig[widget.category]?.[widget.index]
  if (!currentWidget) return config

  nextConfig[widget.category][widget.index] = {
    ...currentWidget,
    ...normalizeColorFields(updates),
  }

  return nextConfig
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

  const widgetUpdates = Object.entries(updatesById).filter(
    ([, updates]) => updates && typeof updates === 'object',
  )
  if (!widgetUpdates.length) {
    return config
  }

  const widgetsById = Object.fromEntries(
    buildConfigWidgets(config).map((widget) => [widget.id, widget]),
  )
  const nextConfig = cloneConfig(config)
  let hasChanges = false

  widgetUpdates.forEach(([widgetId, updates]) => {
    const widget = widgetsById[widgetId]
    const currentWidget = widget
      ? nextConfig[widget.category]?.[widget.index]
      : null

    if (!widget || !currentWidget) {
      return
    }

    nextConfig[widget.category][widget.index] = {
      ...currentWidget,
      ...normalizeColorFields(updates),
    }
    hasChanges = true
  })

  return hasChanges ? nextConfig : config
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
  if (!config) return config

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  if (!nextConfig[widget.category]?.[widget.index]) return config

  nextConfig[widget.category][widget.index] = nextWidgetData
  return nextConfig
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

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  nextConfig[widget.category] = (nextConfig[widget.category] || []).filter(
    (_, index) => index !== widget.index,
  )

  return nextConfig
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

  const idsToDelete = new Set(widgetIds)
  const indexesByCategory = buildConfigWidgets(config).reduce(
    (accumulator, widget) => {
      if (!idsToDelete.has(widget.id)) {
        return accumulator
      }

      if (!accumulator[widget.category]) {
        accumulator[widget.category] = new Set()
      }

      accumulator[widget.category].add(widget.index)
      return accumulator
    },
    {},
  )

  const categories = Object.keys(indexesByCategory)
  if (!categories.length) {
    return config
  }

  const nextConfig = cloneConfig(config)
  categories.forEach((category) => {
    const indexes = indexesByCategory[category]
    nextConfig[category] = (nextConfig[category] || []).filter(
      (_, index) => !indexes.has(index),
    )
  })

  return nextConfig
}
