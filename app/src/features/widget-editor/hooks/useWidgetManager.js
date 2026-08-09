/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, and CRUD operations for widget management.
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { TYPE_LABELS } from '@/lib/widget/widget-icons'
import { deleteWidgetInConfig, ensureWidgetIdsInConfig, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget/widget-config'
import { buildConfigWidgets, groupWidgetsForSidebar } from '@/lib/widget/widget-presentation'
import { isStandardMetricWidgetType } from '@/lib/widget/standard-metrics'
import { clamp } from '@/lib/utils'
import { createBackdropDefaults, createLabelDefaults, createMetricValueDefaults, createPlotDefaults, parseInteger } from '../utils/widgetUtils'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'
import { updateLiveWidgetDraft } from '@/features/overlay-editor/utils/widgetDomHelpers'
import { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'

/**
 * Container hook for SidebarWidgetsTab that owns all store access,
 * derived state, and CRUD operations.
 *
 * @returns {{
 *   config: object,
 *   widgets: Array<object>,
 *   selectedWidgetId: string|null,
 *   updateWidgetData: Function,
 *   updateWidgetSize: Function,
 *   commitWidgetSize: Function,
 *   setNumericField: Function,
 *   addWidget: Function,
 *   deleteWidget: Function,
 *   resetWidget: Function,
 *   setSelectedWidgetId: Function,
 * }}
 */
export function useWidgetManager({ widgetLiveEdits }) {
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
  const liveEdits = useWidgetDraftView(widgetLiveEdits)
  const parsedActivity = useStore.getState().parsedActivity

  // Derived state — group and build the sidebar widget list from config
  const widgets = useMemo(() => {
    return groupWidgetsForSidebar(applyWidgetDrafts(buildConfigWidgets(config), liveEdits.liveWidgetDrafts), TYPE_LABELS)
  }, [config, liveEdits.liveWidgetDrafts])

  // Update handler — applies partial updates to a widget via config utility
  const updateWidgetData = (id, updates) => {
    setConfig(updateWidgetInConfig(config, id, updates))
  }

  const updateWidgetSize = (id, updates) => {
    const widget = widgets.find((item) => item.id === id)
    if (!liveEdits.draftWidgetsRef.current[id]) {
      liveEdits.beginWidgetInteraction(id, 'slider')
    }
    updateLiveWidgetDraft({
      draftWidgetsRef: liveEdits.draftWidgetsRef,
      setLiveWidgetDraft: liveEdits.setLiveWidgetDraft,
      widgetId: id,
      widget,
      updates,
      target: liveEdits.getWidgetNode(id),
      globalScale: globalDefaults?.scale ?? 1,
    })
  }

  const commitWidgetSize = (id) => {
    const draft = liveEdits.draftWidgetsRef.current[id]?.data
    if (draft) {
      setConfig(updateWidgetInConfig(config, id, draft))
    }

    liveEdits.clearWidgetDraft(id)
    liveEdits.endWidgetInteraction(id)
  }

  // Numeric field handler — parses raw input, clamps to range, updates widget
  const setNumericField = (widgetId, key, rawValue, options = {}) => {
    const { fallback = 0, min, max } = options
    const parsed = parseInteger(rawValue, fallback)
    const nextValue = min !== undefined || max !== undefined ? clamp(parsed, min ?? parsed, max ?? parsed) : parsed

    updateWidgetData(widgetId, { [key]: nextValue })
  }

  // Add widget — creates a new widget of the given type with defaults and appends to config
  const addWidget = ({ type, displayType, lapTimerMode }) => {
    const nextConfig = structuredClone(config)
    let targetCategory = null

    if (type === 'backdrop') {
      if (!nextConfig.backdrops) nextConfig.backdrops = []
      nextConfig.backdrops.push(createBackdropDefaults(displayType))
      targetCategory = 'backdrops'
    } else if (type === 'label') {
      if (!nextConfig.labels) nextConfig.labels = []
      nextConfig.labels.push(createLabelDefaults(globalDefaults))
      targetCategory = 'labels'
    } else if (isStandardMetricWidgetType(type) || ['gradient', 'time'].includes(type)) {
      if (!nextConfig.values) nextConfig.values = []
      nextConfig.values.push(createMetricValueDefaults(type, globalDefaults, { displayType, lapTimerMode }))
      targetCategory = 'values'
    } else if (['course', 'elevation'].includes(type)) {
      if (!nextConfig.plots) nextConfig.plots = []
      nextConfig.plots.push(
        createPlotDefaults(type, globalDefaults, {
          coursePoints: parsedActivity?.sample_course_points,
          sceneFontSize: globalDefaults?.font_size,
        }),
      )
      targetCategory = 'plots'
    }

    const normalizedConfig = ensureWidgetIdsInConfig(nextConfig)
    const newId = targetCategory ? normalizedConfig[targetCategory]?.at(-1)?.id || null : null

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

    if (widget.type === 'backdrop') {
      setConfig(replaceWidgetInConfig(config, id, createBackdropDefaults()))
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

    const selection = widget.type === 'lap_timer' ? { lapTimerMode: 'current_lap' } : {}
    setConfig(replaceWidgetInConfig(config, id, createMetricValueDefaults(widget.type, globalDefaults, selection)))
  }

  return {
    config,
    widgets,
    selectedWidgetId,
    updateWidgetData,
    updateWidgetSize,
    commitWidgetSize,
    setNumericField,
    addWidget,
    deleteWidget,
    resetWidget,
    setSelectedWidgetId,
  }
}
