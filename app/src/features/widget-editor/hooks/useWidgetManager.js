/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, and CRUD operations for widget management.
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { getCurrentParsedActivity } from '@/lib/activity/cache'
import { TYPE_LABELS } from '@/lib/widget-icons'
import {
  buildConfigWidgets,
  deleteWidgetInConfig,
  ensureWidgetIdsInConfig,
  groupWidgetsForSidebar,
  replaceWidgetInConfig,
  updateWidgetInConfig,
} from '@/lib/widget-config'
import { isStandardMetricWidgetType } from '@/lib/standard-metrics'
import { clamp } from '@/lib/utils'
import { createLabelDefaults, createMetricValueDefaults, createPlotDefaults, parseInteger } from '../utils/widgetUtils'
import { HEADING_DEFAULTS } from '../data/widgetDefaults'

/**
 * Container hook for SidebarWidgetsTab that owns all store access,
 * derived state, and CRUD operations.
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

    if (type === 'label') {
      if (!nextConfig.labels) nextConfig.labels = []
      nextConfig.labels.push(createLabelDefaults(globalDefaults))
    } else if (isStandardMetricWidgetType(type) || ['gradient', 'time'].includes(type)) {
      if (!nextConfig.values) nextConfig.values = []
      nextConfig.values.push(createMetricValueDefaults(type, globalDefaults))
    } else if (['course', 'elevation'].includes(type)) {
      if (!nextConfig.plots) nextConfig.plots = []
      nextConfig.plots.push(
        createPlotDefaults(type, globalDefaults, {
          coursePoints: parsedActivity?.sample_course_points,
          sceneFontSize: nextConfig.scene?.font_size,
        }),
      )
    } else if (type === 'heading') {
      if (!nextConfig.plots) nextConfig.plots = []
      nextConfig.plots.push({
        value: 'heading',
        ...HEADING_DEFAULTS,
        opacity: globalDefaults?.opacity ?? 1,
      })
    }

    const normalizedConfig = ensureWidgetIdsInConfig(nextConfig)
    const newId = buildConfigWidgets(normalizedConfig).at(-1)?.id || null

    setConfig(normalizedConfig)
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

    if (widget.type === 'heading') {
      setConfig(
        replaceWidgetInConfig(config, id, {
          value: 'heading',
          ...HEADING_DEFAULTS,
          opacity: globalDefaults?.opacity ?? 1,
        }),
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
