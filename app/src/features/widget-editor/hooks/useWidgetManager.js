/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, and CRUD operations for widget management.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { deleteWidgetInConfig, ensureWidgetIdsInConfig, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget/widget-config'
import { buildConfigWidgets, getWidgetTypeName, groupWidgetsForSidebar, withAltitudeEditorPresentation } from '@/lib/widget/widget-presentation'
import { isStandardMetricWidgetType } from '@/lib/widget/standard-metrics'
import { RASTER_DEFAULTS } from '@/lib/widget/standard-widgets'
import { clamp } from '@/lib/utils'
import { createBackdropDefaults, createLabelDefaults, createMetricValueDefaults, createPlotDefaults, parseInteger } from '../utils/widgetUtils'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'
import { updateLiveWidgetDraft } from '@/features/overlay-editor/utils/widgetDomHelpers'
import { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { useFontPreparation } from '@/hooks/useFonts'
import { collectFontIds, createLabelFontSelection } from '@/lib/fonts'
import { getPreparedFont } from '@/lib/font-resources'

const CONTENT_ALIGNMENT_FACTORS = {
  left: 0,
  center: 0.5,
  right: 1,
}

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
  const prepareFontEdit = useFontPreparation()

  // Derived state — group and build the sidebar widget list from config
  const widgets = useMemo(() => {
    const configWidgets = applyWidgetDrafts(buildConfigWidgets(config), liveEdits.liveWidgetDrafts)
    const presentedWidgets = configWidgets.map((widget) => withAltitudeEditorPresentation(widget, parsedActivity))
    return groupWidgetsForSidebar(presentedWidgets, (type) => getWidgetTypeName(type, t))
  }, [config, liveEdits.liveWidgetDrafts, parsedActivity, t])

  // Update handler — applies partial updates to a widget via config utility
  const updateWidgetData = (id, updates) => {
    const widget = widgets.find((item) => item.id === id)
    let nextUpdates = updates

    if (Object.hasOwn(updates, 'content_alignment') && updates.content_alignment !== widget?.data.content_alignment) {
      const renderedContentWidth = Number(liveEdits.getWidgetNode(id)?.dataset.widgetContentWidth)
      const currentAlignmentFactor = CONTENT_ALIGNMENT_FACTORS[widget.data.content_alignment]
      const nextAlignmentFactor = CONTENT_ALIGNMENT_FACTORS[updates.content_alignment]
      if (currentAlignmentFactor === undefined || nextAlignmentFactor === undefined) throw new Error('Unsupported content alignment')
      if (!Number.isFinite(renderedContentWidth) || renderedContentWidth < 0) throw new Error('Alignment change requires rendered content width')
      nextUpdates = {
        ...updates,
        x: widget.data.x + (nextAlignmentFactor - currentAlignmentFactor) * renderedContentWidth,
      }
    }

    const fontIds = collectFontIds(updates, widget.data)
    if (Object.hasOwn(updates, 'font') && !fontIds.includes(updates.font)) fontIds.push(updates.font)
    const commit = () => {
      const state = useStore.getState()
      const currentWidget = buildConfigWidgets(state.config).find((item) => item.id === id)
      if (!currentWidget) return // A pending font edit may outlive a deleted widget.
      const selection =
        currentWidget.category === 'labels' && Object.hasOwn(updates, 'font')
          ? createLabelFontSelection(getPreparedFont(updates.font), currentWidget.data)
          : {}
      state.setConfig(updateWidgetInConfig(state.config, id, { ...nextUpdates, ...selection }))
    }
    if (fontIds.length) prepareFontEdit(id, fontIds, commit)
    else commit()
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
    let category
    let data
    if (type === 'backdrop') {
      category = 'backdrops'
      data = createBackdropDefaults(displayType)
    } else if (type === 'raster') {
      category = 'rasters'
      data = { ...RASTER_DEFAULTS }
    } else if (type === 'label') {
      category = 'labels'
      data = createLabelDefaults(globalDefaults)
    } else if (isStandardMetricWidgetType(type) || ['gradient', 'time'].includes(type)) {
      category = 'values'
      data = createMetricValueDefaults(type, globalDefaults, { displayType, lapTimerMode })
    } else if (['course', 'elevation'].includes(type)) {
      category = 'plots'
      data = createPlotDefaults(type, globalDefaults, {
        coursePoints: parsedActivity?.sample_course_points,
        sceneFontSize: globalDefaults.font_size,
      })
    } else throw new Error(`Unknown widget type: ${type}`)

    prepareFontEdit(Symbol('add widget'), collectFontIds(data), () => {
      const state = useStore.getState()
      const nextConfig = ensureWidgetIdsInConfig({ ...state.config, [category]: [...state.config[category], data] })
      state.setConfig(nextConfig)
      state.setSelectedWidgetId(nextConfig[category].at(-1).id)
    })
  }

  // Delete widget — removes the widget by id and updates config
  const deleteWidget = (id) => {
    setConfig(deleteWidgetInConfig(config, id))
  }

  // Reset widget — replaces widget data with fresh defaults for its type
  const resetWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    const commitReset = (data) =>
      prepareFontEdit(id, collectFontIds(data), () => {
        const state = useStore.getState()
        if (!buildConfigWidgets(state.config).some((item) => item.id === id)) return
        state.setConfig(replaceWidgetInConfig(state.config, id, data))
      })

    if (widget.type === 'label') return commitReset(createLabelDefaults(globalDefaults))

    if (widget.type === 'backdrop') {
      commitReset(createBackdropDefaults())
      return
    }

    if (widget.type === 'raster') {
      commitReset({ ...RASTER_DEFAULTS })
      return
    }

    if (widget.type === 'course' || widget.type === 'elevation') {
      commitReset(createPlotDefaults(widget.type, globalDefaults, { sceneFontSize: config.scene.font_size }))
      return
    }

    const selection = widget.type === 'lap_timer' ? { lapTimerMode: 'current_lap' } : {}
    commitReset(createMetricValueDefaults(widget.type, globalDefaults, selection))
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
