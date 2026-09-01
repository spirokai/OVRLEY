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
import { convertAltitudeValue, getElevationProfileSeries, getFirstAltitudeValue, getPreferredElevationSeries } from './altitude'

/**
 * Flattens the config's backdrop/label/value/plot arrays into a uniform widget list
 * with consistent { id, type, category, index, name, data } entries.
 *
 * @param {*} config - Overlay template configuration data.
 * @returns {object[]} Uniform widget array for editor consumption.
 */
export function buildConfigWidgets(config) {
  if (!config) return []

  const normalizedConfig = ensureWidgetIdsInConfig(config)
  const widgets = []

  ;(normalizedConfig.backdrops || []).forEach((item, index) => {
    widgets.push({ id: item.id, type: 'backdrop', category: 'backdrops', index, name: 'Backdrop', data: item })
  })
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
 * Adds an activity-derived placeholder without changing configured altitude state.
 * @param {object} widget
 * @param {object|null} activity
 * @returns {object}
 */
export function withAltitudeEditorPresentation(widget, activity) {
  if (!['altitude', 'elevation'].includes(widget.type) || (widget.data.starting_altitude !== null && widget.data.starting_altitude !== undefined)) {
    return widget
  }

  const series = widget.type === 'altitude' ? getPreferredElevationSeries(activity) : getElevationProfileSeries(activity)
  const firstValue = getFirstAltitudeValue(series ?? [])
  if (firstValue === null) return widget

  const unit = widget.type === 'altitude' ? widget.data.display_unit : widget.data.starting_altitude_unit
  return { ...widget, startingAltitudePlaceholder: Math.round(convertAltitudeValue(firstValue, 'm', unit)) }
}

/**
 * Groups widgets by type name (using typeLabels lookup) and sorts
 * groups alphabetically. The first widget in each group gets a groupLabel;
 * subsequent widgets in the same group get null.
 *
 * @param {object[]} widgets - Uniform widget list from buildConfigWidgets.
 * @param {(type: string) => string} getTypeName - Resolves a configured widget type to its display name.
 * @returns {object[]} Widgets with groupLabel annotations.
 */
export function groupWidgetsForSidebar(widgets, getTypeName) {
  const grouped = widgets.reduce((accumulator, widget) => {
    const typeName = getTypeName(widget.type)
    if (!accumulator[typeName]) accumulator[typeName] = []
    accumulator[typeName].push({ ...widget, name: widget.type === 'label' ? widget.name : typeName })
    return accumulator
  }, {})

  const sortTypeNames = (left, right) => {
    if (left === 'Backdrop') return -1
    if (right === 'Backdrop') return 1
    return left.localeCompare(right)
  }

  return Object.keys(grouped)
    .sort(sortTypeNames)
    .flatMap((typeName) =>
      grouped[typeName].map((widget, widgetIndex) => ({
        ...widget,
        groupLabel: widgetIndex === 0 ? typeName : null,
      })),
    )
}
