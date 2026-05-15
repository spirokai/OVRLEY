/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, side effects, and CRUD operations for widget management.
 */

import { useEffect, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { getCurrentParsedActivity } from '@/lib/activity/cache'
import { buildConfigWidgets, deleteWidgetInConfig, groupWidgetsForSidebar, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget-config'
import { TYPE_LABELS } from '../data/widgetDefinitions'
import { createLabelDefaults, createMetricValueDefaults, createPlotDefaults, clamp, parseInteger } from '../utils/widgetUtils'

/**
 * Container hook for SidebarWidgetsTab that owns all store access,
 * derived state, side effects, and CRUD operations.
 *
 * @returns {{
 *   config: object,
 *   widgets: Array<object>,
 *   selectedWidgetId: string|null,
 *   updateWidgetData: Function,
 *   setNumericField: Function,
 *   addWidget: Function,
 *   deleteWidget: Function,
 *   resetWidget: Function,
 *   setSelectedWidgetId: Function,
 * }}
 */
export function useWidgetManager() {
  // Store selectors — shallow-pick zustand state needed for widget management
  const { config, globalDefaults, selectedWidgetId, setConfig, setSelectedWidgetId } = useStore(
    useShallow((state) => ({
      config: state.config,
      globalDefaults: state.globalDefaults,
      selectedWidgetId: state.selectedWidgetId,
      setConfig: state.setConfig,
      setSelectedWidgetId: state.setSelectedWidgetId,
    })),
  )
  const parsedActivity = getCurrentParsedActivity()

  // Derived state — group and build the sidebar widget list from config
  const widgets = useMemo(() => {
    return groupWidgetsForSidebar(buildConfigWidgets(config), TYPE_LABELS)
  }, [config])

  // Side effect — sync selectedWidgetId when widgets change (removal, empty state, initial selection)
  useEffect(() => {
    if (widgets.length === 0) {
      if (selectedWidgetId !== null) setSelectedWidgetId(null)
      return
    }

    if (!selectedWidgetId) {
      setSelectedWidgetId(widgets[widgets.length - 1].id)
      return
    }

    if (!widgets.some((widget) => widget.id === selectedWidgetId)) {
      setSelectedWidgetId(widgets[0].id)
    }
  }, [widgets, selectedWidgetId, setSelectedWidgetId])

  // Update handler — applies partial updates to a widget via config utility
  const updateWidgetData = (id, updates) => {
    setConfig(updateWidgetInConfig(config, id, updates))
  }

  // Numeric field handler — parses raw input, clamps to range, updates widget
  const setNumericField = (widgetId, key, rawValue, options = {}) => {
    const { fallback = 0, min, max } = options
    const parsed = parseInteger(rawValue, fallback)
    const nextValue = min !== undefined || max !== undefined ? clamp(parsed, min ?? parsed, max ?? parsed) : parsed

    updateWidgetData(widgetId, { [key]: nextValue })
  }

  // Add widget — creates a new widget of the given type with defaults and appends to config
  const addWidget = (type) => {
    const nextConfig = structuredClone(config)
    let newId = ''

    if (type === 'label') {
      if (!nextConfig.labels) nextConfig.labels = []
      nextConfig.labels.push(createLabelDefaults(globalDefaults))
      newId = `label-${nextConfig.labels.length - 1}`
    } else if (['speed', 'gradient', 'heartrate', 'power', 'cadence', 'time', 'temperature'].includes(type)) {
      if (!nextConfig.values) nextConfig.values = []
      nextConfig.values.push(createMetricValueDefaults(type, globalDefaults))
      newId = `value-${nextConfig.values.length - 1}`
    } else if (['course', 'elevation'].includes(type)) {
      if (!nextConfig.plots) nextConfig.plots = []
      nextConfig.plots.push(
        createPlotDefaults(type, globalDefaults, {
          coursePoints: parsedActivity?.sample_course_points,
          sceneFontSize: nextConfig.scene?.font_size,
        }),
      )
      newId = `plot-${nextConfig.plots.length - 1}`
    }

    setConfig(nextConfig)
    if (newId) setSelectedWidgetId(newId)
  }

  // Delete widget — removes the widget by id and updates config
  const deleteWidget = (id) => {
    setConfig(deleteWidgetInConfig(config, id))
  }

  // Reset widget — replaces widget data with fresh defaults for its type
  const resetWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    if (widget.type === 'label') {
      setConfig(replaceWidgetInConfig(config, id, createLabelDefaults(globalDefaults)))
      return
    }

    if (widget.type === 'course' || widget.type === 'elevation') {
      setConfig(
        replaceWidgetInConfig(
          config,
          id,
          createPlotDefaults(widget.type, globalDefaults, {
            sceneFontSize: config?.scene?.font_size,
          }),
        ),
      )
      return
    }

    setConfig(replaceWidgetInConfig(config, id, createMetricValueDefaults(widget.type, globalDefaults)))
  }

  return {
    config,
    widgets,
    selectedWidgetId,
    updateWidgetData,
    setNumericField,
    addWidget,
    deleteWidget,
    resetWidget,
    setSelectedWidgetId,
  }
}
