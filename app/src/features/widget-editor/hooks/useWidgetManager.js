/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, and CRUD operations for widget management.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { getWidgetTypeName } from '@/lib/widget/widget-icons'
import { deleteWidgetInConfig, ensureWidgetIdsInConfig, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget/widget-config'
import { buildConfigWidgets, groupWidgetsForSidebar, withAltitudeEditorPresentation } from '@/lib/widget/widget-presentation'
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
  const { t } = useTranslation()
  // Store selectors — shallow-pick zustand state needed for widget management
  const { config, globalDefaults, parsedActivity, selectedWidgetId, setConfig, setSelectedWidgetId } = useStore(
    useShallow((state) => ({
      config: state.config,
      globalDefaults: state.globalDefaults,
      parsedActivity: state.parsedActivity,
      selectedWidgetId: state.selectedWidgetId,
      setConfig: state.setConfig,
      setSelectedWidgetId: state.setSelectedWidgetId,
    })),
  )
  const liveEdits = useWidgetDraftView(widgetLiveEdits)

  // Derived state — group and build the sidebar widget list from config
  const widgets = useMemo(() => {
    const configWidgets = applyWidgetDrafts(buildConfigWidgets(config), liveEdits.liveWidgetDrafts)
    const presentedWidgets = configWidgets.map((widget) => withAltitudeEditorPresentation(widget, parsedActivity))
    return groupWidgetsForSidebar(presentedWidgets, (type) => getWidgetTypeName(type, t))
  }, [config, liveEdits.liveWidgetDrafts, parsedActivity, t])

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
    const { fallback = 0, min, max, optional = false, integer = true, round = false } = options
    if (optional && rawValue === '') {
      updateWidgetData(widgetId, { [key]: null })
      return
    }
    let parsed = integer ? parseInteger(rawValue, fallback) : Number(rawValue)
    if (round) parsed = Math.round(Number(rawValue))
    if (!Number.isFinite(parsed)) throw new Error(`Invalid numeric value for ${key}`)
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
      nextConfig.values.push(
        createMetricValueDefaults(type, globalDefaults, {
          displayType,
          lapTimerMode,
        }),
      )
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
