/** Display variant initialization, projection, geometry, and canonical data patches. */

import { pickDefined } from '@/lib/utils'
import { DISPLAY_TYPE_DEFINITIONS, requireDisplayMode } from './widget-catalog'

/** @param {object|undefined} data Current data or an absent draft. @param {object} updates Partial data. @returns {object} Data with variants merged by display type. */
export function mergeWidgetData(data, updates) {
  const next = { ...data, ...updates }
  if (updates.display_variants !== undefined) {
    next.display_variants = { ...data?.display_variants }
    for (const [type, fields] of Object.entries(updates.display_variants))
      next.display_variants[type] = { ...data?.display_variants?.[type], ...fields }
  }
  return next
}

/** @param {object} data Canonical widget data; display_type is absent for widgets without display choices. @returns {object} Active display projected for editing and rendering. */
export function resolveActiveWidgetData(data) {
  if (data.display_type === undefined || DISPLAY_TYPE_DEFINITIONS[data.display_type].layoutMode === 'intrinsic') return data
  const definition = DISPLAY_TYPE_DEFINITIONS[data.display_type]
  const resolved = { ...data, ...data.display_variants[data.display_type] }
  for (const [target, source] of Object.entries(definition.resolvedFrame)) resolved[target] = resolved[source]
  return resolved
}

/**
 * Initialize optional variant fields at creation, document ingress, or display selection.
 * @param {object} data Widget data.
 * @param {string} displayType Display to initialize.
 * @param {object|null} frameFallback Document geometry; null uses manifest geometry.
 * @returns {object} Data with the initialized variant.
 */
export function initDisplayVariant(data, displayType, frameFallback = data) {
  const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
  if (definition.layoutMode === 'intrinsic') return data
  const variant = {
    ...definition.variantDefaults,
    ...pickDefined(frameFallback, definition.geometryKeys),
    ...data.display_variants?.[displayType],
  }
  for (const [target, source] of Object.entries(definition.inheritedFields)) variant[target] ??= data[source]
  return { ...data, display_variants: { ...data.display_variants, [displayType]: variant } }
}

/** @param {object} data Widget data. @param {object} geometry Gesture/inspector geometry. @returns {object} Patch following the display's geometry contract. */
export function buildFrameGeometryUpdate(data, geometry) {
  const definition = DISPLAY_TYPE_DEFINITIONS[data.display_type]
  if (definition.layoutMode === 'intrinsic') return geometry
  const fields = {}
  for (const [target, sources] of Object.entries(definition.geometryBindings)) {
    for (const source of sources) {
      if (geometry[source] !== undefined) {
        fields[target] = geometry[source]
        break
      }
    }
  }
  return {
    ...pickDefined(geometry, definition.geometryFields),
    display_variants: mergeWidgetData(data, { display_variants: { [data.display_type]: fields } }).display_variants,
  }
}

/** @param {object} data Widget data. @param {string} displayType Selected display. @returns {object} Display patch restoring its geometry and shared defaults. */
export function buildDisplayTypeChangeUpdate(data, displayType) {
  const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
  const initialized = initDisplayVariant(data, displayType, null)
  return {
    display_type: displayType,
    display_variants: initialized.display_variants,
    ...definition.defaults,
    ...pickDefined(initialized.display_variants?.[displayType], definition.geometryFields),
  }
}

/** @param {string} displayType Active display. @param {string} value Selected mode. @returns {object} Mode-owned labels and typography defaults. */
export function buildDisplayModeUpdate(displayType, value) {
  return { ...requireDisplayMode(displayType, value).defaults }
}

function applyWidgetDraft(drafts, widget) {
  const draft = drafts[widget.id]
  if (draft === undefined) return widget
  return { ...widget, data: resolveActiveWidgetData(mergeWidgetData(widget.data, draft)) }
}

/** @param {object[]} widgets Config widgets. @param {object} drafts Data patches keyed by ID. @returns {object[]} Widgets with live data applied. */
export function applyWidgetDrafts(widgets, drafts) {
  return widgets.map(applyWidgetDraft.bind(null, drafts))
}
