/** Widget document ingress, immutable edits, resource identities, and selection. */

import { cloneSerializable } from '@/store/store-utils'
import { produce } from 'immer'
import { normalizeColorFields } from '../color-utils'
import { mergeWidgetData } from './widget-resolver'
import { WIDGET_COLLECTIONS } from './widget-catalog'

const GENERATED_WIDGET_ID_PATTERN = /^widget-(\d+)$/

function getStartingGeneratedId(config) {
  let nextId = 1
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    for (const widget of config[category]) {
      const match = GENERATED_WIDGET_ID_PATTERN.exec(widget.id)
      if (match) nextId = Math.max(nextId, Number(match[1]) + 1)
    }
  }
  return nextId
}

/**
 * Document ingress: absent collections become empty and absent IDs are assigned.
 * Present collections and IDs must be valid; duplicate IDs are rejected.
 * @param {object|null} config Incoming document, or null when no document is open.
 * @returns {object|null} Document with canonical collections and identities.
 */
export const ensureWidgetIdsInConfig = produce(function ensureWidgetIdsInConfig(config) {
  if (config === null) return null
  if (typeof config !== 'object' || Array.isArray(config)) throw new Error('Widget config must be an object')
  const usedIds = new Set()
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    if (config[category] === undefined) config[category] = []
    if (!Array.isArray(config[category])) throw new Error(`Invalid widget collection: ${category}`)
    for (const widget of config[category]) {
      if (widget.id === undefined) continue
      if (typeof widget.id !== 'string' || widget.id.trim() === '') throw new Error('Invalid widget ID')
      if (usedIds.has(widget.id)) throw new Error(`Duplicate widget ID: ${widget.id}`)
      usedIds.add(widget.id)
    }
  }
  let nextId = getStartingGeneratedId(config)
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    for (const widget of config[category]) {
      if (widget.id === undefined) widget.id = `widget-${nextId++}`
    }
  }
})

function findWidgetInConfig(config, widgetId) {
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    for (let index = 0; index < config[category].length; index += 1) {
      if (config[category][index].id === widgetId) return { category, index }
    }
  }
  throw new Error(`Unknown widget ID: ${widgetId}`)
}

/** @param {object} config Canonical document. @param {string} widgetId Widget ID. @param {object} updates Partial data. @returns {object} Edited document. */
export function updateWidgetInConfig(config, widgetId, updates) {
  return updateWidgetsInConfig(config, { [widgetId]: updates })
}

/** @param {object} config Canonical document. @param {object} updatesById Partial data keyed by ID. @returns {object} Atomically edited document. */
export const updateWidgetsInConfig = produce(function updateWidgetsInConfig(config, updatesById) {
  for (const [widgetId, updates] of Object.entries(updatesById)) {
    const { category, index } = findWidgetInConfig(config, widgetId)
    config[category][index] = { ...mergeWidgetData(config[category][index], normalizeColorFields(updates)), id: widgetId }
  }
})

/** @param {object} config Canonical document. @param {string} widgetId Widget ID. @param {object} data Replacement data. @returns {object} Edited document. */
export const replaceWidgetInConfig = produce(function replaceWidgetInConfig(config, widgetId, data) {
  const { category, index } = findWidgetInConfig(config, widgetId)
  config[category][index] = { ...data, id: widgetId }
})

/** @param {object} config Canonical document. @param {number} factor Relative scale. @param {object|null} origin Fixed point; null for an empty document. @returns {object} Scaled document. */
export const scaleWidgetPositions = produce(function scaleWidgetPositions(config, factor, origin) {
  if (factor === 1) return
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    for (const widget of config[category]) {
      widget.x = origin.left + (widget.x - origin.left) * factor
      widget.y = origin.top + (widget.y - origin.top) * factor
    }
  }
})

/** @param {object} config Canonical document. @param {string[]} widgetIds Widget IDs. @returns {object} Document with the widgets removed. */
export const deleteWidgetsInConfig = produce(function deleteWidgetsInConfig(config, widgetIds) {
  const ids = new Set(widgetIds)
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    const collection = config[category]
    for (let index = collection.length - 1; index >= 0; index -= 1) {
      if (ids.has(collection[index].id)) collection.splice(index, 1)
    }
  }
})

const appendDuplicatedWidgets = produce(function appendDuplicatedWidgets(config, widgets, { offsetX = 24, offsetY = 24 }) {
  let nextId = getStartingGeneratedId(config)
  for (const widget of widgets) {
    if (!Object.hasOwn(WIDGET_COLLECTIONS, widget.category)) throw new Error(`Unknown widget collection: ${widget.category}`)
    const data = cloneSerializable(widget.data)
    data.id = `widget-${nextId++}`
    data.x += offsetX
    data.y += offsetY
    config[widget.category].push(data)
  }
})

/**
 * @param {object} config Canonical document.
 * @param {Array<{category: string, data: object}>} widgetsToDuplicate Clipboard widgets.
 * @param {{offsetX?: number, offsetY?: number}} options Optional paste offsets.
 * @returns {{config: object, insertedWidgetIds: string[]}} Document and new identities in document order.
 */
export function duplicateWidgetsInConfig(config, widgetsToDuplicate, options = {}) {
  const nextConfig = appendDuplicatedWidgets(config, widgetsToDuplicate, options)
  const insertedWidgetIds = []
  for (const category of Object.keys(WIDGET_COLLECTIONS)) {
    for (const widget of nextConfig[category].slice(config[category].length)) insertedWidgetIds.push(widget.id)
  }
  return { config: nextConfig, insertedWidgetIds }
}

/** @param {string[]} widgetIds Requested IDs. @param {string[]} orderedWidgetIds Document order. @returns {string[]} Existing IDs in document order. */
export function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const ids = new Set(widgetIds)
  const selectedIds = []
  for (const id of orderedWidgetIds) {
    if (ids.has(id)) selectedIds.push(id)
  }
  return selectedIds
}

/** @param {string[]} widgetIds Selected IDs. @param {string|null} preferredId Preferred primary selection. @returns {string|null} Primary ID or null for no selection. */
export function getPrimarySelectionId(widgetIds, preferredId = null) {
  return preferredId !== null && widgetIds.includes(preferredId) ? preferredId : (widgetIds.at(-1) ?? null)
}

/** @param {object} config Document being loaded. @param {object} results Rust resource results by widget ID. @returns {object} Document with resource identities. */
export function attachRasterLoadResults(config, results) {
  const rasters = []
  for (const raster of config.rasters) {
    if (raster.path === null) {
      rasters.push(raster)
      continue
    }
    const resource = results[raster.id]
    if (!resource) throw new Error(`Raster ${raster.id} has no resource result`)
    if (resource.status === 'ready') rasters.push({ ...raster, resourceId: resource.resourceId })
    else if (resource.status === 'error') rasters.push({ ...raster, resourceErrorCode: resource.errorCode })
    else throw new Error(`Raster ${raster.id} has an invalid resource result`)
  }
  return { ...config, rasters }
}

/** @param {object} config Canonical document. @returns {object} Rust handles keyed by widget ID for project archives. */
export function rasterResourceIds(config) {
  const ids = {}
  for (const raster of config.rasters) {
    if (raster.path === null) continue
    if (!raster.resourceId) throw new Error(`Raster ${raster.id} has no loaded image resource`)
    ids[raster.id] = raster.resourceId
  }
  return ids
}
