/**
 * @file useOverlayEditorState – Derived state hook for the overlay editor.
 *
 * Owns the store selectors and derived state for widgets, scene dimensions,
 * preview values, global defaults, and widget capability flags. Does NOT
 * own selection management (see useWidgetSelection.js), keyboard shortcuts
 * (see useEditorKeyboard.js), viewport tracking (see useEditorViewport.js),
 * pointer handling (see createOverlayPointerHandlers.js), or moveable
 * interaction handlers.
 *
 * Those concerns are composed at the component level in OverlayEditor.jsx
 * so each hook is independently testable and replaceable.
 *
 * @module useOverlayEditorState
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import useStore from '@/store/useStore'
import { useActivityContext, useVideoTimeline } from '@/hooks/useVideoTimeline'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { updateWidgetInConfig, updateWidgetsInConfig } from '@/lib/widget/widget-config'
import { getEffectiveWidgetData } from '@/lib/template/template-state'
import { applyWidgetDrafts, applyWidgetDraftsForCanvas } from '@/lib/widget/widget-draft'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'
import { getSceneSize } from '../utils/overlayEditorUtils'
import useWidgetDraftState, { useWidgetDraftView } from './useWidgetDraftState'

function materializeWidgets(rawWidgets, globalDefaults) {
  return rawWidgets.map((widget) => ({ ...widget, data: getEffectiveWidgetData(widget, globalDefaults) }))
}

export function useOverlayEditorStateWithLiveEdits({ config, globalDefaults, onConfigChange }, widgetLiveEdits) {
  const selectedSecond = useStore((state) => state.selectedSecond)
  const exportRange = useStore((state) => state.renderSettings.range)
  const activityContext = useActivityContext()
  const activity = activityContext?.activity ?? null
  const activityOriginSecond = activityContext?.originSecond ?? 0
  const activityClipVideoId = activityContext?.videoId ?? null
  const timeline = useVideoTimeline()
  const activityClipDurationSeconds =
    activityClipVideoId === null ? null : timeline.clips.find((clip) => clip.id === activityClipVideoId).durationSeconds
  const videoBounds = timeline.bounds

  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)

  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})
  const liveDraftState = useWidgetDraftView(widgetLiveEdits)
  const {
    activeWidgetInteraction,
    beginWidgetInteraction,
    clearWidgetDraft,
    clearWidgetDrafts,
    draftWidgetsRef,
    liveWidgetDrafts,
    resetWidgetDrafts,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
    endWidgetInteraction,
  } = liveDraftState

  const setWidgetNode = useCallback(
    (widgetId, node) => {
      widgetLiveEdits.setWidgetNode(widgetId, node)
      setWidgetNodes((current) => {
        if (node && current[widgetId] === node) return current
        if (!node && !current[widgetId]) return current

        const next = { ...current }
        if (node) next[widgetId] = node
        else delete next[widgetId]
        return next
      })
    },
    [widgetLiveEdits],
  )

  const rawWidgets = useMemo(() => buildConfigWidgets(config), [config])
  const widgets = useMemo(() => materializeWidgets(rawWidgets, globalDefaults), [globalDefaults, rawWidgets])
  const sceneSize = useMemo(() => getSceneSize(config), [config])
  const globalOpacity = globalDefaults.opacity
  const globalScale = globalDefaults.scale
  const sceneStyle = useMemo(
    () => ({
      border_color: globalDefaults?.border_color ?? config?.scene?.border_color,
      border_thickness: globalDefaults?.border_thickness ?? config?.scene?.border_thickness ?? 0,
      shadow_color: globalDefaults?.shadow_color ?? config?.scene?.shadow_color,
      shadow_strength: globalDefaults?.shadow_strength ?? config?.scene?.shadow_strength ?? 0,
      shadow_distance: globalDefaults?.shadow_distance ?? config?.scene?.shadow_distance ?? 0,
    }),
    [globalDefaults, config?.scene],
  )
  // Widgets read activity time: timeline seconds shifted by the activity origin. Without an
  // external activity the clip at the playhead supplies its own telemetry from its start.
  const previewExportRange = useMemo(() => {
    if (exportRange.type === 'custom') {
      return { ...exportRange, from: exportRange.from - activityOriginSecond, to: exportRange.to - activityOriginSecond }
    }
    if (activityClipDurationSeconds !== null) return { type: 'custom', from: 0, to: activityClipDurationSeconds }
    if (videoBounds === null) return exportRange
    return { type: 'custom', from: videoBounds.startSecond, to: videoBounds.endSecond }
  }, [activityClipDurationSeconds, activityOriginSecond, exportRange, videoBounds])
  const previewExportStartSecond = previewExportRange.from

  useEffect(() => {
    incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
  }, [selectedSecond])

  useEffect(() => {
    resetWidgetDrafts()
  }, [config, resetWidgetDrafts])

  const renderedWidgets = useMemo(() => applyWidgetDrafts(widgets, liveWidgetDrafts), [liveWidgetDrafts, widgets])
  const canvasWidgets = useMemo(() => applyWidgetDraftsForCanvas(widgets, liveWidgetDrafts), [liveWidgetDrafts, widgets])
  const renderedWidgetMap = useMemo(() => Object.fromEntries(renderedWidgets.map((w) => [w.id, w])), [renderedWidgets])
  const orderedWidgetIds = useMemo(() => renderedWidgets.map((w) => w.id), [renderedWidgets])

  const widgetRefCallbacks = useMemo(
    () =>
      Object.fromEntries(
        widgets.map((widget) => [
          widget.id,
          (node) => {
            setWidgetNode(widget.id, node)
          },
        ]),
      ),
    [setWidgetNode, widgets],
  )

  const commitWidgetUpdate = (widgetId, updates) => {
    if (!config) return
    onConfigChange(updateWidgetInConfig(config, widgetId, updates))
  }
  const commitWidgetUpdates = (updatesById) => {
    if (!config) return
    onConfigChange(updateWidgetsInConfig(config, updatesById))
  }

  return {
    activeWidgetInteraction,
    beginWidgetInteraction,
    clearWidgetDraft,
    clearWidgetDrafts,
    commitWidgetUpdate,
    commitWidgetUpdates,
    config,
    canvasWidgets,
    draftWidgetsRef,
    globalDefaults,
    globalOpacity,
    globalScale,
    activity,
    interactionStartRef,
    liveWidgetDrafts,
    moveableRef,
    onConfigChange,
    orderedWidgetIds,
    previewExportRange,
    previewExportStartSecond,
    previewSecond: selectedSecond - activityOriginSecond,
    renderedWidgetMap,
    renderedWidgets,
    sceneElement,
    sceneSize,
    sceneStyle,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
    endWidgetInteraction,
    setSceneElement,
    widgetNodes,
    widgetRefCallbacks,
    widgets,
  }
}

export default function useOverlayEditorState(options) {
  const widgetLiveEdits = useWidgetDraftState()
  return useOverlayEditorStateWithLiveEdits(options, widgetLiveEdits)
}
