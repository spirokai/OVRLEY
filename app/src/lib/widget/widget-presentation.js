/** Prepare widget lists, translated names, sidebar order, and activity-derived placeholders. */

import { convertAltitudeValue, getStartingAltitude } from './altitude'
import { WIDGET_COLLECTIONS, WIDGET_TYPE_DEFINITIONS, requireWidgetTypeDefinition } from './widget-catalog'

/**
 * Returns a translated activity attribute label, including attributes without a widget definition.
 * @param {string} type Activity attribute identifier.
 * @param {import('i18next').TFunction} translate Translation function.
 * @returns {string} Activity attribute label.
 */
export function getActivityAttributeLabel(type, translate) {
  const definition = WIDGET_TYPE_DEFINITIONS[type]
  if (definition) return translate(definition.nameKey)

  const words = []
  for (const part of type.split('_')) words.push(part === 'gps' ? 'GPS' : part.charAt(0).toUpperCase() + part.slice(1))
  const defaultLabel = words.join(' ')
  return translate(`widgets.activityAttributes.${type}`, defaultLabel)
}

/**
 * Returns the translated name of a supported widget.
 * @param {string} type Widget type.
 * @param {import('i18next').TFunction} translate Translation function.
 * @returns {string} Widget name.
 */
export function getWidgetTypeName(type, translate) {
  const definition = requireWidgetTypeDefinition(type)
  return translate(definition.nameKey)
}

/**
 * Flattens the config's backdrop/label/value/plot arrays into a uniform widget list
 * with consistent { id, type, category, name, data } entries.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {object[]} Uniform widget array for editor consumption.
 */
export function buildConfigWidgets(config) {
  if (config === null) return []
  const widgets = []

  for (const [category, collection] of Object.entries(WIDGET_COLLECTIONS)) {
    for (const data of config[category]) {
      const type = collection.type ?? data[collection.typeField]
      const definition = requireWidgetTypeDefinition(type)
      widgets.push({ id: data.id, type, category, name: definition.nameField === null ? type : data[definition.nameField], data })
    }
  }

  return widgets
}

/**
 * Adds an activity-derived placeholder without changing configured altitude state.
 * @param {object} widget
 * @param {object|null} activity
 * @returns {object}
 */
export function withAltitudeEditorPresentation(widget, activity) {
  const source = WIDGET_TYPE_DEFINITIONS[widget.type].altitudeSource
  if (source === null || (widget.data.starting_altitude !== null && widget.data.starting_altitude !== undefined)) {
    return widget
  }

  const firstValue = getStartingAltitude(activity, source)
  if (firstValue === null) return widget

  return { ...widget, startingAltitudePlaceholder: Math.round(convertAltitudeValue(firstValue, 'm', widget.data[source.unitField])) }
}

/**
 * Groups and names sidebar widgets, preserving label text and sorting groups alphabetically.
 *
 * @param {object[]} widgets - Uniform widget list from buildConfigWidgets.
 * @param {(type: string) => string} getTypeName - Resolves a configured widget type to its display name.
 * @returns {object[]} Named widgets in sidebar order.
 */
export function groupWidgetsForSidebar(widgets, getTypeName) {
  const grouped = {}
  for (const widget of widgets) {
    const typeName = getTypeName(widget.type)
    const definition = requireWidgetTypeDefinition(widget.type)
    if (!grouped[widget.type]) grouped[widget.type] = { name: typeName, priority: definition.sidebarPriority, widgets: [] }
    grouped[widget.type].widgets.push({ ...widget, name: definition.nameField === null ? typeName : widget.name })
  }
  const sorted = []
  for (const group of Object.values(grouped).sort(sortWidgetGroups)) sorted.push(...group.widgets)
  return sorted
}

function sortWidgetGroups(left, right) {
  return left.priority - right.priority || left.name.localeCompare(right.name)
}
