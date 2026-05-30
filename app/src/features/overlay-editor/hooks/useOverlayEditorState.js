/**
 * Container hook for OverlayEditor — orchestrates store access, derived state,
 * selection management, viewport tracking, keyboard shortcuts, and sub-hooks.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import useStore from '@/store/useStore'
import { buildConfigWidgets, updateWidgetInConfig, updateWidgetsInConfig } from '@/lib/widget-config'
import { getEffectiveWidgetData } from '@/lib/template-state'
import { deepEqual } from '@/store/store-utils'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'
import { useDragHandlers } from './useDragHandlers'
import { useResizeHandlers } from './useResizeHandlers'
import { useScaleHandlers } from './useScaleHandlers'
import { useRotateHandlers } from './useRotateHandlers'
import useOverlayPointerHandlers from '../utils/createOverlayPointerHandlers'
import { getPrimarySelectionId, normalizeSelectionIds } from '../utils/overlayEditorHelpers'
import { clamp } from '@/lib/utils'
import { getSceneSize } from '../utils/overlayEditorUtils'
import { useEditorViewport } from './useEditorViewport'
import { useEditorKeyboard } from './useEditorKeyboard'
import useWidgetDraftState from './useWidgetDraftState'

/**
 * Clamps the preview second to valid activity duration bounds.
 */
function resolvePreviewSecond({ dummyDurationSeconds, selectedSecond, sourceActivity }) {
  const rawSecond = Number(selectedSecond) || 0
  const activityDuration = Number(sourceActivity?.trim_end_seconds ?? sourceActivity?.metadata?.duration_seconds ?? dummyDurationSeconds ?? 0)
  const maxSecond = Math.max(Number.isFinite(activityDuration) ? activityDuration : 0, 0)

  return clamp(rawSecond, 0, maxSecond)
}

/**
 * Formats seconds as HH:MM:SS for helpers that consume export-range-shaped windows.
 */
function formatRangeTime(seconds) {
  const safeSeconds = Math.max(0, Math.trunc(Number(seconds) || 0))
  const hours = Math.floor(safeSeconds / 3600)
  const minutes = Math.floor((safeSeconds % 3600) / 60)
  const remainingSeconds = safeSeconds % 60

  return [hours, minutes, remainingSeconds].map((part) => String(part).padStart(2, '0')).join(':')
}

/**
 * Merges live widget drafts (unsaved edits) into the widget array.
 * Each draft's data properties override the corresponding widget's data.
 */
function mergeDraftsIntoWidgets(widgets, liveWidgetDrafts) {
  return widgets.map((widget) => {
    const draft = liveWidgetDrafts[widget.id]
    if (!draft) {
      return widget
    }

    return {
      ...widget,
      data: {
        ...widget.data,
        ...draft,
      },
    }
  })
}

export default function useOverlayEditorState({ config, globalDefaults, onConfigChange, zoomLevel, onZoomLevelChange }) {
  // Store selectors — shallow-pick zustand state needed for overlay editor
  const selectedWidgetId = useStore((state) => state.selectedWidgetId)
  const selectedWidgetIds = useStore((state) => state.selectedWidgetIds)
  const setWidgetSelection = useStore((state) => state.setWidgetSelection)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const dummyDurationSeconds = useStore((state) => state.dummyDurationSeconds)
  const exportRange = useStore((state) => state.exportRange)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoDuration = useStore((state) => state.importedVideoDuration)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)

  // Refs — mutable interaction state
  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)
  const scalePreviewFrameRef = useRef(null)
  const marqueeCleanupRef = useRef(null)
  const marqueeSelectionRef = useRef(null)
  const prevWidgetDataRef = useRef(null)

  // Local UI state — group drag, scene element, widget nodes, and marquee UI
  const [isGroupDragActive, setIsGroupDragActive] = useState(false)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})
  const [selectionRect, setSelectionRect] = useState(null)

  // Widget draft state — live edits during drag/resize/scale/rotate
  const { clearWidgetDraft, clearWidgetDrafts, draftWidgetsRef, liveWidgetDrafts, resetWidgetDrafts, setLiveWidgetDraft, setLiveWidgetDraftsBatch } =
    useWidgetDraftState()

  // Derived state — computed values from store and props
  const sourceActivity = useStore.getState().parsedActivity
  const rawWidgets = useMemo(() => buildConfigWidgets(config), [config])
  const widgets = useMemo(
    () =>
      rawWidgets.map((widget) => ({
        ...widget,
        data: getEffectiveWidgetData(widget, globalDefaults),
      })),
    [globalDefaults, rawWidgets],
  )
  const sceneSize = useMemo(() => getSceneSize(config), [config])
  const activity = sourceActivity
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
    [
      globalDefaults?.border_color,
      globalDefaults?.border_thickness,
      globalDefaults?.shadow_color,
      globalDefaults?.shadow_distance,
      globalDefaults?.shadow_strength,
      config?.scene?.border_color,
      config?.scene?.border_thickness,
      config?.scene?.shadow_color,
      config?.scene?.shadow_distance,
      config?.scene?.shadow_strength,
    ],
  )
  const previewSecond = useMemo(() => {
    return resolvePreviewSecond({
      dummyDurationSeconds,
      selectedSecond,
      sourceActivity,
    })
  }, [dummyDurationSeconds, selectedSecond, sourceActivity])
  const previewExportRange = useMemo(() => {
    if (!importedVideoPath) {
      return exportRange
    }

    const duration = Number(importedVideoDuration)
    if (!Number.isFinite(duration) || duration <= 0) {
      return null
    }

    const start = Math.max(0, Number(videoSyncOffsetSeconds) || 0)
    const end = start + duration
    return {
      type: 'custom',
      fromTime: formatRangeTime(start),
      toTime: formatRangeTime(end),
    }
  }, [exportRange, importedVideoDuration, importedVideoPath, videoSyncOffsetSeconds])

  // Side effects — preview perf counter, draft reset on config change, marquee cleanup
  useEffect(() => {
    incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
  }, [previewSecond])

  useEffect(() => {
    resetWidgetDrafts()
    setIsGroupDragActive(false)
    setGroupDragSelectionIds([])
  }, [config, resetWidgetDrafts])

  useEffect(
    () => () => {
      marqueeCleanupRef.current?.()
    },
    [],
  )

  // Viewport — resize observer and fit scale computation
  const { viewportRef, fitScale } = useEditorViewport(sceneSize)

  const displayScale = fitScale * zoomLevel
  const renderedWidgets = useMemo(() => mergeDraftsIntoWidgets(widgets, liveWidgetDrafts), [liveWidgetDrafts, widgets])
  const renderedWidgetMap = useMemo(() => Object.fromEntries(renderedWidgets.map((widget) => [widget.id, widget])), [renderedWidgets])
  const orderedWidgetIds = useMemo(() => renderedWidgets.map((widget) => widget.id), [renderedWidgets])

  // Selection management — the store owns canonical selection state while the
  // editor passes intentful updates (single-select, toggle, marquee).
  const setSelectionState = useCallback(
    (widgetIds) => {
      setWidgetSelection(normalizeSelectionIds(widgetIds, orderedWidgetIds))
    },
    [orderedWidgetIds, setWidgetSelection],
  )

  const commitSelection = useCallback(
    (widgetIds, preferredId = null) => {
      const normalizedIds = normalizeSelectionIds(widgetIds, orderedWidgetIds)
      setWidgetSelection(normalizedIds, getPrimarySelectionId(normalizedIds, preferredId))
    },
    [orderedWidgetIds, setWidgetSelection],
  )

  const effectiveSelectedWidgetIds = useMemo(
    () => (isGroupDragActive ? normalizeSelectionIds(groupDragSelectionIds, orderedWidgetIds) : selectedWidgetIds),
    [groupDragSelectionIds, isGroupDragActive, orderedWidgetIds, selectedWidgetIds],
  )

  // Selected widget derivation — primary selection, targets, guidelines
  const selectedWidgets = useMemo(
    () => effectiveSelectedWidgetIds.map((widgetId) => renderedWidgetMap[widgetId]).filter(Boolean),
    [effectiveSelectedWidgetIds, renderedWidgetMap],
  )
  const primarySelectedWidgetId = getPrimarySelectionId(effectiveSelectedWidgetIds, selectedWidgetId)
  const selectedWidget = primarySelectedWidgetId ? renderedWidgetMap[primarySelectedWidgetId] || null : null
  const [widgetDataVersion, setWidgetDataVersion] = useState(0)

  const selectedWidgetData = useMemo(() => selectedWidgets.map((widget) => widget?.data ?? null), [selectedWidgets])

  useLayoutEffect(() => {
    const prev = prevWidgetDataRef.current

    if (!prev || !deepEqual(selectedWidgetData, prev)) {
      setWidgetDataVersion((v) => v + 1)
    }
    prevWidgetDataRef.current = selectedWidgetData
  }, [selectedWidgetData])

  const selectedWidgetDataSignature = widgetDataVersion
  const isGroupSelection = effectiveSelectedWidgetIds.length > 1
  const selectedTarget = !isGroupSelection && primarySelectedWidgetId ? widgetNodes[primarySelectedWidgetId] || null : null
  const selectedTargets = useMemo(
    () => (isGroupSelection ? effectiveSelectedWidgetIds.map((widgetId) => widgetNodes[widgetId]).filter(Boolean) : []),
    [effectiveSelectedWidgetIds, isGroupSelection, widgetNodes],
  )
  const elementGuidelines = useMemo(
    () =>
      widgets
        .filter((widget) => !effectiveSelectedWidgetIds.includes(widget.id))
        .map((widget) => widgetNodes[widget.id])
        .filter(Boolean),
    [effectiveSelectedWidgetIds, widgetNodes, widgets],
  )

  // Moveable rect update — re-measure after display/scale changes
  useEffect(() => {
    if (!moveableRef.current || (!selectedTarget && selectedTargets.length === 0)) {
      return undefined
    }

    const frameId = requestAnimationFrame(() => {
      moveableRef.current?.updateRect()
    })

    return () => cancelAnimationFrame(frameId)
  }, [displayScale, globalScale, selectedTarget, selectedTargets, selectedWidgetDataSignature])

  // Editor capability flags
  const canResizeSelected = !isGroupSelection && selectedWidget?.category === 'plots'
  const showEdgeResizeHandles = canResizeSelected && selectedWidget?.type === 'elevation'
  const canScaleSelected = Boolean(!isGroupSelection && selectedWidget && selectedWidget.category !== 'plots')
  const canRotateSelected = !isGroupSelection && selectedWidget?.type === 'course'
  const maintainAspectRatio = !isGroupSelection && (selectedWidget?.type === 'course' || canScaleSelected)

  // Widget ref callbacks — register/unregister DOM nodes
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
              if (node) {
                next[widget.id] = node
              } else {
                delete next[widget.id]
              }
              return next
            })
          },
        ]),
      ),
    [widgets],
  )

  // Config mutation helpers
  const commitWidgetUpdate = (widgetId, updates) => {
    if (!config) return
    onConfigChange(updateWidgetInConfig(config, widgetId, updates))
  }

  const commitWidgetUpdates = (updatesById) => {
    if (!config) return
    onConfigChange(updateWidgetsInConfig(config, updatesById))
  }

  // Keyboard — Delete key removes selected widgets
  useEditorKeyboard({
    config,
    onConfigChange,
    selectedWidgetIds,
    setWidgetSelection,
  })

  // Pointer handlers — scene mousedown, widget mousedown, wheel zoom
  const { handleSceneMouseDown, handleWidgetMouseDown, handleWheel } = useOverlayPointerHandlers({
    commitSelection,
    displayScale,
    moveableRef,
    marqueeCleanupRef,
    marqueeSelectionRef,
    onZoomLevelChange,
    orderedWidgetIds,
    sceneElement,
    sceneSize,
    selectedWidgetId,
    selectedWidgetIds,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setSelectionRect,
    setSelectionState,
    widgetNodes,
  })

  // Moveable handlers — drag, resize, scale, rotate
  const dragHandlers = useDragHandlers({
    clearWidgetDraft,
    clearWidgetDrafts,
    commitWidgetUpdate,
    commitWidgetUpdates,
    draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale,
    groupDragSelectionIds,
    interactionStartRef,
    renderedWidgetMap,
    previewSecond,
    scalePreviewFrameRef,
    selectedTarget,
    selectedWidget,
    selectedWidgets,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
  })
  const resizeHandlers = useResizeHandlers({
    clearWidgetDraft,
    commitWidgetUpdate,
    draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale,
    interactionStartRef,
    renderedWidgetMap,
    previewSecond,
    scalePreviewFrameRef,
    selectedTarget,
    selectedWidget,
    setLiveWidgetDraft,
  })
  const scaleHandlers = useScaleHandlers({
    clearWidgetDraft,
    commitWidgetUpdate,
    draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale,
    interactionStartRef,
    renderedWidgetMap,
    previewSecond,
    scalePreviewFrameRef,
    selectedTarget,
    selectedWidget,
    setLiveWidgetDraft,
  })
  const rotateHandlers = useRotateHandlers({
    clearWidgetDraft,
    commitWidgetUpdate,
    draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale,
    interactionStartRef,
    renderedWidgetMap,
    previewSecond,
    scalePreviewFrameRef,
    selectedTarget,
    selectedWidget,
    setLiveWidgetDraft,
  })
  const handlers = {
    ...dragHandlers,
    ...resizeHandlers,
    ...scaleHandlers,
    ...rotateHandlers,
  }

  // Return — full editor state object consumed by OverlayEditor component
  return {
    activity,
    canResizeSelected,
    canRotateSelected,
    canScaleSelected,
    displayScale,
    elementGuidelines,
    exportRange: previewExportRange,
    globalOpacity,
    globalScale,
    handleSceneMouseDown,
    handleWheel,
    handleWidgetMouseDown,
    handlers,
    maintainAspectRatio,
    moveableRef,
    previewSecond,
    sceneElement,
    sceneStyle,
    sceneSize,
    isGroupDragActive,
    selectedTarget,
    selectedTargets,
    selectedWidget,
    selectedWidgetIds,
    selectionRect,
    setSceneElement,
    showEdgeResizeHandles,
    viewportRef,
    widgetRefCallbacks,
    widgets: renderedWidgets,
  }
}
