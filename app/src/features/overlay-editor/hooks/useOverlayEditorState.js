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

import { useEffect, useMemo, useRef, useState } from 'react'
import useStore from '@/store/useStore'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { updateWidgetInConfig, updateWidgetsInConfig } from '@/lib/widget/widget-config'
import { resolvePreviewSecond } from '@/lib/preview-timing'
import { getEffectiveWidgetData } from '@/lib/template/template-state'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'
import { getSceneSize } from '../utils/overlayEditorUtils'
import useWidgetDraftState from './useWidgetDraftState'

function formatExportRangeTime(seconds) {
  const safeSeconds = Math.max(0, Math.trunc(Number(seconds) || 0))
  const hours = Math.floor(safeSeconds / 3600)
  const minutes = Math.floor((safeSeconds % 3600) / 60)
  const remainingSeconds = safeSeconds % 60
  return [hours, minutes, remainingSeconds].map((part) => String(part).padStart(2, '0')).join(':')
}

function mergeDraftsIntoWidgets(widgets, liveWidgetDrafts) {
  return widgets.map((widget) => {
    const draft = liveWidgetDrafts[widget.id]
    if (!draft) return widget
    return { ...widget, data: { ...widget.data, ...draft } }
  })
}

export default function useOverlayEditorState({ config, globalDefaults, onConfigChange, zoomLevel, onZoomLevelChange }) {
  const selectedSecond = useStore((state) => state.selectedSecond)
  const dummyDurationSeconds = useStore((state) => state.dummyDurationSeconds)
  const exportRange = useStore((state) => state.exportRange)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoDuration = useStore((state) => state.importedVideoDuration)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)

  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)
  const scalePreviewFrameRef = useRef(null)

  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})

  const {
    clearWidgetDraft,
    clearWidgetDrafts,
    draftWidgetsRef,
    liveWidgetDrafts,
    liveWidgetPreviews,
    resetWidgetDrafts,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
    setLiveWidgetPreview,
  } = useWidgetDraftState()

  const sourceActivity = useStore.getState().parsedActivity
  const rawWidgets = useMemo(() => buildConfigWidgets(config), [config])
  const widgets = useMemo(
    () => rawWidgets.map((widget) => ({ ...widget, data: getEffectiveWidgetData(widget, globalDefaults) })),
    [globalDefaults, rawWidgets],
  )
  const sceneSize = useMemo(() => getSceneSize(config), [config])
  const globalOpacity = globalDefaults?.opacity ?? 1
  const globalScale = globalDefaults?.scale ?? 1
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
  const previewSecond = useMemo(
    () => resolvePreviewSecond({ dummyDurationSeconds, selectedSecond, sourceActivity }),
    [dummyDurationSeconds, selectedSecond, sourceActivity],
  )
  const previewExportRange = useMemo(() => {
    if (!importedVideoPath) return exportRange
    const duration = Number(importedVideoDuration)
    if (!Number.isFinite(duration) || duration <= 0) return null
    const start = Math.max(0, Number(videoSyncOffsetSeconds) || 0)
    const end = start + duration
    return { type: 'custom', fromTime: formatExportRangeTime(start), toTime: formatExportRangeTime(end) }
  }, [exportRange, importedVideoDuration, importedVideoPath, videoSyncOffsetSeconds])

  useEffect(() => {
    incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
  }, [previewSecond])

  useEffect(() => {
    resetWidgetDrafts()
  }, [config, resetWidgetDrafts])

  const renderedWidgets = useMemo(() => mergeDraftsIntoWidgets(widgets, liveWidgetDrafts), [liveWidgetDrafts, widgets])
  const renderedWidgetMap = useMemo(() => Object.fromEntries(renderedWidgets.map((w) => [w.id, w])), [renderedWidgets])
  const orderedWidgetIds = useMemo(() => renderedWidgets.map((w) => w.id), [renderedWidgets])

  const widgetRefCallbacks = useMemo(
    () =>
      Object.fromEntries(
        widgets.map((widget) => [
          widget.id,
          (node) => {
            setWidgetNodes((current) => {
              if (node && current[widget.id] === node) return current
              if (!node && !current[widget.id]) return current
              const next = { ...current }
              if (node) next[widget.id] = node
              else delete next[widget.id]
              return next
            })
          },
        ]),
      ),
    [widgets],
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
    clearWidgetDraft,
    clearWidgetDrafts,
    commitWidgetUpdate,
    commitWidgetUpdates,
    config,
    draftWidgetsRef,
    globalDefaults,
    globalOpacity,
    globalScale,
    interactionStartRef,
    liveWidgetDrafts,
    liveWidgetPreviews,
    moveableRef,
    onConfigChange,
    onZoomLevelChange,
    orderedWidgetIds,
    previewExportRange,
    previewSecond,
    renderedWidgetMap,
    renderedWidgets,
    sceneElement,
    sceneSize,
    sceneStyle,
    scalePreviewFrameRef,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
    setLiveWidgetPreview,
    setSceneElement,
    widgetNodes,
    widgetRefCallbacks,
    widgets,
    zoomLevel,
  }
}
