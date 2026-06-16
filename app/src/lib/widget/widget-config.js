/**
 * @file widget-config – Widget identity management and config mutation (CRUD).
 *
 * Owns all config-level widget operations: ID assignment, find, update
 * (single and batch), replace, and delete. Does NOT own sidebar presentation
 * concerns — those live in the sibling module widget-presentation.js.
 *
 * What this module owns:
 * - ensureWidgetIdsInConfig, findWidgetInConfig
 * - updateWidgetInConfig, updateWidgetsInConfig, replaceWidgetInConfig
 * - deleteWidgetInConfig, deleteWidgetsInConfig
 *
 * What widget-presentation.js owns:
 * - buildConfigWidgets, groupWidgetsForSidebar
 *
 * @module widget-config
 */

import { cloneSerializable } from '@/store/store-utils'
import { normalizeColorFields } from '../color-utils'

const LEGACY_WIDGET_ID_PATTERN = /^(label|value|plot)-\d+$/
const GENERATED_WIDGET_ID_PATTERN = /^widget-(\d+)$/
const WIDGET_ID_PREFIX = 'widget-'
const WIDGET_CATEGORIES = ['labels', 'values', 'plots']

/**
 * Returns whether a widget id is durable enough to preserve across saves.
 *
 * Legacy index-derived ids are treated as unstable even if they are present in
 * widget data, because they still encode array position.
 *
 * @param {*} widgetId - Identifier to validate.
 * @returns {boolean} Whether the id can be preserved as-is.
 */
function isDurableWidgetId(widgetId) {
  if (typeof widgetId !== 'string') {
    return false
  }

  const normalizedId = widgetId.trim()
  if (!normalizedId) {
    return false
  }

  return !LEGACY_WIDGET_ID_PATTERN.test(normalizedId)
}

function getNextGeneratedId(usedIds, nextIdRef) {
  let nextWidgetId = `${WIDGET_ID_PREFIX}${nextIdRef.current}`
  while (usedIds.has(nextWidgetId)) {
    nextIdRef.current += 1
    nextWidgetId = `${WIDGET_ID_PREFIX}${nextIdRef.current}`
  }

  usedIds.add(nextWidgetId)
  nextIdRef.current += 1
  return nextWidgetId
}

function getStartingGeneratedId(config) {
  let maxGeneratedId = 0

  WIDGET_CATEGORIES.forEach((category) => {
    const collection = config?.[category]
    if (!Array.isArray(collection)) {
      return
    }

    collection.forEach((widget) => {
      const match = GENERATED_WIDGET_ID_PATTERN.exec(String(widget?.id || ''))
      if (!match) {
        return
      }

      maxGeneratedId = Math.max(maxGeneratedId, Number.parseInt(match[1], 10) || 0)
    })
  })

  return maxGeneratedId + 1
}

/**
 * Ensures every persisted widget carries a unique durable id.
 *
 * The helper upgrades legacy templates on contact by copying the config only
 * when a widget is missing an id, carries an index-derived legacy id, or
 * collides with another widget's id.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {*} Config with durable widget ids.
 */
export function ensureWidgetIdsInConfig(config) {
  if (!config) {
    return config
  }

  const usedIds = new Set()
  const nextIdRef = { current: getStartingGeneratedId(config) }
  let nextConfig = config

  WIDGET_CATEGORIES.forEach((category) => {
    const collection = config[category]
    if (!Array.isArray(collection)) {
      return
    }

    let nextCollection = null

    collection.forEach((widget, index) => {
      const currentId = widget?.id
      const nextId = isDurableWidgetId(currentId) && !usedIds.has(currentId) ? currentId : getNextGeneratedId(usedIds, nextIdRef)

      if (nextId !== currentId) {
        if (!nextCollection) {
          nextCollection = [...collection]
        }

        nextCollection[index] = {
          ...widget,
          id: nextId,
        }
      } else {
        usedIds.add(nextId)
      }
    })

    if (nextCollection) {
      if (nextConfig === config) {
        nextConfig = { ...config }
      }

      nextConfig[category] = nextCollection
    }
  })

  return nextConfig
}

/**
 * Finds a widget by durable id inside the config's widget collections.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} widgetId - Identifier of the target widget.
 * @returns {object|null} Resolved widget location and data.
 */
export function findWidgetInConfig(config, widgetId) {
  const normalizedConfig = ensureWidgetIdsInConfig(config)

  for (const category of WIDGET_CATEGORIES) {
    const currentCollection = normalizedConfig?.[category]
    if (!Array.isArray(currentCollection)) {
      continue
    }

    const index = currentCollection.findIndex((widget) => widget?.id === widgetId)
    if (index === -1) {
      continue
    }

    return {
      category,
      config: normalizedConfig,
      data: currentCollection[index],
      id: widgetId,
      index,
    }
  }

  return null
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

  const target = findWidgetInConfig(config, widgetId)
  if (!target) {
    return config
  }

  const currentCollection = target.config[target.category]
  if (!Array.isArray(currentCollection)) {
    return config
  }

  const currentWidget = currentCollection[target.index]
  if (!currentWidget) {
    return config
  }

  const nextWidget = updater(currentWidget, target)
  if (!nextWidget || nextWidget === currentWidget) {
    return target.config
  }

  const nextCollection = [...currentCollection]
  nextCollection[target.index] = nextWidget

  return {
    ...target.config,
    [target.category]: nextCollection,
  }
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
  const normalizedUpdates = normalizeColorFields(updates)
  return updateWidgetEntry(config, widgetId, (currentWidget) => ({
    ...currentWidget,
    ...normalizedUpdates,
    id: currentWidget.id,
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

  const normalizedConfig = ensureWidgetIdsInConfig(config)
  const widgetUpdates = Object.entries(updatesById).filter(([, updates]) => updates && typeof updates === 'object')
  if (!widgetUpdates.length) {
    return normalizedConfig
  }

  const nextCollectionsByCategory = new Map()
  let hasChanges = false

  widgetUpdates.forEach(([widgetId, updates]) => {
    const target = findWidgetInConfig(normalizedConfig, widgetId)
    if (!target) {
      return
    }

    const sourceCollection = nextCollectionsByCategory.has(target.category)
      ? nextCollectionsByCategory.get(target.category)
      : normalizedConfig[target.category]
    const currentWidget = Array.isArray(sourceCollection) ? sourceCollection[target.index] : null

    if (!currentWidget) {
      return
    }

    const nextWidget = {
      ...currentWidget,
      ...normalizeColorFields(updates),
      id: currentWidget.id,
    }
    const nextCollection = [...sourceCollection]
    nextCollection[target.index] = nextWidget
    nextCollectionsByCategory.set(target.category, nextCollection)
    hasChanges = true
  })

  if (!hasChanges) {
    return normalizedConfig
  }

  return {
    ...normalizedConfig,
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
  return updateWidgetEntry(config, widgetId, (currentWidget) => ({
    ...nextWidgetData,
    id: currentWidget.id,
  }))
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

  const target = findWidgetInConfig(config, widgetId)
  if (!target) {
    return config
  }

  const currentCollection = target.config[target.category]
  if (!Array.isArray(currentCollection) || !currentCollection[target.index]) {
    return config
  }

  return {
    ...target.config,
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

  const normalizedConfig = ensureWidgetIdsInConfig(config)
  const indexesByCategory = widgetIds.reduce((accumulator, widgetId) => {
    const target = findWidgetInConfig(normalizedConfig, widgetId)
    if (!target || !normalizedConfig?.[target.category]?.[target.index]) {
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
    normalizedConfig,
  )
}

/**
 * Duplicates the provided widgets into the config and returns their new ids.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {Array<{ category: string, data: object }>} widgetsToDuplicate - Widgets to duplicate.
 * @param {object} [options] - Duplication options.
 * @param {number} [options.offsetX=24] - Horizontal paste offset.
 * @param {number} [options.offsetY=24] - Vertical paste offset.
 * @returns {{ config: object, insertedWidgetIds: string[] }} Updated config and duplicated widget ids.
 */
export function duplicateWidgetsInConfig(config, widgetsToDuplicate, options = {}) {
  if (!config || !Array.isArray(widgetsToDuplicate) || !widgetsToDuplicate.length) {
    return { config, insertedWidgetIds: [] }
  }

  const { offsetX = 24, offsetY = 24 } = options
  const previousWidgetIds = new Set(
    WIDGET_CATEGORIES.flatMap((category) => (Array.isArray(config?.[category]) ? config[category].map((widget) => widget?.id).filter(Boolean) : [])),
  )
  const nextConfig = {
    ...config,
    labels: [...(config.labels || [])],
    values: [...(config.values || [])],
    plots: [...(config.plots || [])],
  }

  widgetsToDuplicate.forEach((widget) => {
    if (!WIDGET_CATEGORIES.includes(widget?.category) || !widget?.data) {
      return
    }

    const duplicatedWidget = cloneSerializable(widget.data)
    delete duplicatedWidget.id
    duplicatedWidget.x = (Number(duplicatedWidget.x) || 0) + offsetX
    duplicatedWidget.y = (Number(duplicatedWidget.y) || 0) + offsetY
    nextConfig[widget.category].push(duplicatedWidget)
  })

  const normalizedConfig = ensureWidgetIdsInConfig(nextConfig)
  const insertedWidgetIds = WIDGET_CATEGORIES.flatMap((category) =>
    (normalizedConfig[category] || []).map((widget) => widget?.id).filter((widgetId) => widgetId && !previousWidgetIds.has(widgetId)),
  )

  return {
    config: normalizedConfig,
    insertedWidgetIds,
  }
}
