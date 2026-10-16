/**
 * Container hook for SidebarWidgetsTab.
 * Owns store selectors, derived state, and CRUD operations for widget management.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { requireWidgetTypeDefinition } from '@/lib/widget/widget-catalog'
import { deleteWidgetsInConfig, ensureWidgetIdsInConfig, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget/widget-config'
import { buildConfigWidgets, getWidgetTypeName, groupWidgetsForSidebar, withAltitudeEditorPresentation } from '@/lib/widget/widget-presentation'
import { clamp } from '@/lib/utils'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { parseInteger } from '../utils/widgetInput'
import { applyWidgetDrafts } from '@/lib/widget/widget-resolver'
import { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { getWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'

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

    const state = useStore.getState()
    state.setConfig(updateWidgetInConfig(state.config, id, nextUpdates))
  }

  const updateWidgetSize = (id, updates) => {
    const widget = widgets.find((item) => item.id === id)
    const tapeUpdate = updates.display_variants?.heading_tape
    const patch = Object.hasOwn(updates, 'rotation')
      ? { ...updates, ...getWidgetInteraction(liveEdits.getWidgetNode(id)).rotate(updates.rotation) }
      : widget.data.display_type === 'heading_tape' && tapeUpdate && Object.hasOwn(tapeUpdate, 'label_offset')
        ? getWidgetInteraction(liveEdits.getWidgetNode(id)).resize({ label_offset: tapeUpdate.label_offset })
        : updates
    liveEdits.setLiveWidgetDraft(id, patch)
  }

  const commitWidgetSize = (id) => {
    const draft = liveEdits.getWidgetDraft(id)
    if (draft) {
      setConfig(updateWidgetInConfig(config, id, draft))
    }

    liveEdits.clearWidgetDraft(id)
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
  const addWidget = (selection) => {
    const category = requireWidgetTypeDefinition(selection.type).collection
    const data = createWidgetDefaults(selection, globalDefaults, {
      coursePoints: parsedActivity?.sample_course_points,
      sceneFontSize: globalDefaults.font_size,
    })

    const state = useStore.getState()
    const nextConfig = ensureWidgetIdsInConfig({ ...state.config, [category]: [...state.config[category], data] })
    state.setConfig(nextConfig)
    state.setSelectedWidgetId(nextConfig[category].at(-1).id)
  }

  // Delete widget — removes the widget by id and updates config
  const deleteWidget = (id) => {
    setConfig(deleteWidgetsInConfig(config, [id]))
  }

  // Reset widget — replaces widget data with fresh defaults for its type
  const resetWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    const data = createWidgetDefaults({ type: widget.type, ...requireWidgetTypeDefinition(widget.type).resetSelection }, globalDefaults, {
      sceneFontSize: config.scene.font_size,
    })
    const state = useStore.getState()
    state.setConfig(replaceWidgetInConfig(state.config, id, data))
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
