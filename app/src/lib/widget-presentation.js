/**
 * @file widget-presentation – Builds and groups widgets for sidebar rendering.
 *
 * These functions are purely concerned with how widgets are displayed in the
 * sidebar (widget drawer, widget editor sidebar tabs). They do NOT mutate
 * config — that responsibility lives in the sibling module widget-config.js.
 *
 * What this module owns:
 * - buildConfigWidgets      flattens config categories into a uniform widget list
 * - groupWidgetsForSidebar  groups and sorts widgets for sidebar display
 *
 * What widget-config.js owns:
 * - Widget CRUD (ensureWidgetIdsInConfig, findWidgetInConfig, updateWidgetInConfig,
 *   updateWidgetsInConfig, replaceWidgetInConfig, deleteWidgetInConfig,
 *   deleteWidgetsInConfig)
 *
 * @module widget-presentation
 */

import { ensureWidgetIdsInConfig } from './widget-config'

/**
 * Flattens the config's label/value/plot arrays into a uniform widget list
 * with consistent { id, type, category, index, name, data } entries.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {object[]} Uniform widget array for editor consumption.
 */
export function buildConfigWidgets(config) {
  if (!config) return []

  const normalizedConfig = ensureWidgetIdsInConfig(config)
  const widgets = []

  ;(normalizedConfig.labels || []).forEach((item, index) => {
    widgets.push({ id: item.id, type: 'label', category: 'labels', index, name: item.text || 'Text', data: item })
  })
  ;(normalizedConfig.values || []).forEach((item, index) => {
    widgets.push({ id: item.id, type: item.value, category: 'values', index, name: item.value, data: item })
  })
  ;(normalizedConfig.plots || []).forEach((item, index) => {
    widgets.push({ id: item.id, type: item.value, category: 'plots', index, name: item.value, data: item })
  })

  return widgets
}

/**
 * Groups widgets by type name (using typeLabels lookup) and sorts
 * groups alphabetically. The first widget in each group gets a groupLabel;
 * subsequent widgets in the same group get null.
 *
 * @param {object[]} widgets - Uniform widget list from buildConfigWidgets.
 * @param {object} typeLabels - Lookup from widget.type to display label.
 * @returns {object[]} Widgets with groupLabel annotations.
 */
export function groupWidgetsForSidebar(widgets, typeLabels) {
  const grouped = widgets.reduce((accumulator, widget) => {
    const typeName = typeLabels[widget.type] || widget.type
    if (!accumulator[typeName]) accumulator[typeName] = []
    accumulator[typeName].push({ ...widget, name: widget.type === 'label' ? widget.name : typeName })
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
