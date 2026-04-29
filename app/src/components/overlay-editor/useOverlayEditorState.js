import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getCurrentParsedActivity } from '../../api/activityCache'
import useStore from '../../store/useStore'
import {
  buildConfigWidgets,
  deleteWidgetsInConfig,
  updateWidgetInConfig,
  updateWidgetsInConfig,
} from '@/lib/widget-config'
import { applyGlobalDefaults } from '@/lib/config-utils'
import useOverlayMoveableHandlers from './createOverlayMoveableHandlers'
import useOverlayPointerHandlers from './createOverlayPointerHandlers'
import {
  getPrimarySelectionId,
  isEditableElement,
  normalizeSelectionIds,
} from './overlayEditorHelpers'
import { clamp, getSceneSize } from './utils'
import useWidgetDraftState from './useWidgetDraftState'

function resolvePreviewSecond({
  dummyDurationSeconds,
  selectedSecond,
  sourceActivity,
}) {
  const rawSecond = Number(selectedSecond) || 0
  const activityDuration = Number(
    sourceActivity?.trim_end_seconds ??
      sourceActivity?.metadata?.duration_seconds ??
      dummyDurationSeconds ??
      0,
  )
  const maxSecond = Math.max(
    Number.isFinite(activityDuration) ? activityDuration : 0,
    0,
  )

  return clamp(rawSecond, 0, maxSecond)
}

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

function selectionIdsChanged(leftIds, rightIds) {
  return (
    leftIds.length !== rightIds.length ||
    leftIds.some((widgetId, index) => widgetId !== rightIds[index])
  )
}

export default function useOverlayEditorState({
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
}) {
  const selectedWidgetId = useStore((state) => state.selectedWidgetId)
  const setSelectedWidgetId = useStore((state) => state.setSelectedWidgetId)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const dummyDurationSeconds = useStore((state) => state.dummyDurationSeconds)
  const viewportRef = useRef(null)
  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)
  const scalePreviewFrameRef = useRef(null)
  const selectionSyncRef = useRef(false)
  const marqueeCleanupRef = useRef(null)
  const marqueeSelectionRef = useRef(null)
  const [isGroupDragActive, setIsGroupDragActive] = useState(false)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })
  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})
  const [selectedWidgetIds, setSelectedWidgetIds] = useState([])
  const [selectionRect, setSelectionRect] = useState(null)
  const {
    clearWidgetDraft,
    clearWidgetDrafts,
    draftWidgetsRef,
    liveWidgetDrafts,
    resetWidgetDrafts,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
  } = useWidgetDraftState()

  const sourceActivity = getCurrentParsedActivity()
  const resolvedConfig = useMemo(
    () =>
      applyGlobalDefaults(config, {
        ...globalDefaults,
        opacity: 1,
        scale: 1,
      }),
    [config, globalDefaults],
  )
  const widgets = useMemo(
    () => buildConfigWidgets(resolvedConfig),
    [resolvedConfig],
  )
  const sceneSize = useMemo(
    () => getSceneSize(resolvedConfig),
    [resolvedConfig],
  )
  const activity = sourceActivity
  const globalOpacity = globalDefaults?.opacity ?? 1
  const globalScale = globalDefaults?.scale ?? 1
  const previewSecond = useMemo(() => {
    return resolvePreviewSecond({
      dummyDurationSeconds,
      selectedSecond,
      sourceActivity,
    })
  }, [dummyDurationSeconds, selectedSecond, sourceActivity])

  useEffect(() => {
    resetWidgetDrafts()
    setIsGroupDragActive(false)
    setGroupDragSelectionIds([])
  }, [resetWidgetDrafts, resolvedConfig])

  useEffect(() => {
    const viewportNode = viewportRef.current
    if (!viewportNode || typeof ResizeObserver === 'undefined') return undefined

    const resizeObserver = new ResizeObserver(([entry]) => {
      const nextWidth = entry?.contentRect?.width || viewportNode.clientWidth
      const nextHeight = entry?.contentRect?.height || viewportNode.clientHeight
      setViewportSize({ width: nextWidth, height: nextHeight })
    })

    resizeObserver.observe(viewportNode)
    return () => resizeObserver.disconnect()
  }, [])

  useEffect(
    () => () => {
      marqueeCleanupRef.current?.()
    },
    [],
  )

  const fitScale = useMemo(() => {
    const safeWidth = Math.max(viewportSize.width - 72, 1)
    const safeHeight = Math.max(viewportSize.height - 72, 1)
    return Math.min(
      safeWidth / sceneSize.width,
      safeHeight / sceneSize.height,
      1,
    )
  }, [viewportSize, sceneSize])

  const displayScale = fitScale * zoomLevel
  const renderedWidgets = useMemo(
    () => mergeDraftsIntoWidgets(widgets, liveWidgetDrafts),
    [liveWidgetDrafts, widgets],
  )
  const renderedWidgetMap = useMemo(
    () =>
      Object.fromEntries(renderedWidgets.map((widget) => [widget.id, widget])),
    [renderedWidgets],
  )
  const orderedWidgetIds = useMemo(
    () => renderedWidgets.map((widget) => widget.id),
    [renderedWidgets],
  )

  const setSelectionState = (widgetIds) => {
    setSelectedWidgetIds(normalizeSelectionIds(widgetIds, orderedWidgetIds))
  }

  const syncPrimarySelectionId = useCallback(
    (nextPrimaryId) => {
      if (selectedWidgetId === nextPrimaryId) {
        selectionSyncRef.current = false
        return
      }

      selectionSyncRef.current = true
      setSelectedWidgetId(nextPrimaryId)
    },
    [selectedWidgetId, setSelectedWidgetId],
  )

  const commitSelection = (widgetIds, preferredId = null) => {
    const normalizedIds = normalizeSelectionIds(widgetIds, orderedWidgetIds)
    const nextPrimaryId = getPrimarySelectionId(normalizedIds, preferredId)

    setSelectedWidgetIds(normalizedIds)
    syncPrimarySelectionId(nextPrimaryId)
  }

  const effectiveSelectedWidgetIds = useMemo(
    () =>
      isGroupDragActive
        ? normalizeSelectionIds(groupDragSelectionIds, orderedWidgetIds)
        : selectedWidgetIds,
    [
      groupDragSelectionIds,
      isGroupDragActive,
      orderedWidgetIds,
      selectedWidgetIds,
    ],
  )

  useEffect(() => {
    if (isGroupDragActive) {
      return
    }

    if (selectionSyncRef.current) {
      selectionSyncRef.current = false
      return
    }

    setSelectedWidgetIds(
      normalizeSelectionIds(
        selectedWidgetId ? [selectedWidgetId] : [],
        orderedWidgetIds,
      ),
    )
  }, [isGroupDragActive, orderedWidgetIds, selectedWidgetId])

  useEffect(() => {
    if (isGroupDragActive) {
      return
    }

    const normalizedIds = normalizeSelectionIds(
      selectedWidgetIds,
      orderedWidgetIds,
    )
    const selectionChanged = selectionIdsChanged(
      normalizedIds,
      selectedWidgetIds,
    )

    if (!selectionChanged) {
      return
    }

    const nextPrimaryId = getPrimarySelectionId(normalizedIds, selectedWidgetId)
    setSelectedWidgetIds(normalizedIds)
    syncPrimarySelectionId(nextPrimaryId)
  }, [
    isGroupDragActive,
    orderedWidgetIds,
    selectedWidgetId,
    selectedWidgetIds,
    setSelectedWidgetId,
    syncPrimarySelectionId,
  ])

  const selectedWidgets = useMemo(
    () =>
      effectiveSelectedWidgetIds
        .map((widgetId) => renderedWidgetMap[widgetId])
        .filter(Boolean),
    [effectiveSelectedWidgetIds, renderedWidgetMap],
  )
  const primarySelectedWidgetId = getPrimarySelectionId(
    effectiveSelectedWidgetIds,
    selectedWidgetId,
  )
  const selectedWidget = primarySelectedWidgetId
    ? renderedWidgetMap[primarySelectedWidgetId] || null
    : null
  const selectedWidgetDataSignature = useMemo(
    () => JSON.stringify(selectedWidgets.map((widget) => widget?.data ?? null)),
    [selectedWidgets],
  )
  const isGroupSelection = effectiveSelectedWidgetIds.length > 1
  const selectedTarget =
    !isGroupSelection && primarySelectedWidgetId
      ? widgetNodes[primarySelectedWidgetId] || null
      : null
  const selectedTargets = useMemo(
    () =>
      isGroupSelection
        ? effectiveSelectedWidgetIds
            .map((widgetId) => widgetNodes[widgetId])
            .filter(Boolean)
        : [],
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

  useEffect(() => {
    if (
      !moveableRef.current ||
      (!selectedTarget && selectedTargets.length === 0)
    ) {
      return undefined
    }

    const frameId = requestAnimationFrame(() => {
      moveableRef.current?.updateRect()
    })

    return () => cancelAnimationFrame(frameId)
  }, [
    displayScale,
    globalScale,
    selectedTarget,
    selectedTargets,
    selectedWidgetDataSignature,
  ])

  const canResizeSelected =
    !isGroupSelection && selectedWidget?.category === 'plots'
  const canScaleSelected = Boolean(
    !isGroupSelection && selectedWidget && selectedWidget.category !== 'plots',
  )
  const canRotateSelected =
    !isGroupSelection && selectedWidget?.type === 'course'
  const maintainAspectRatio =
    !isGroupSelection && (selectedWidget?.type === 'course' || canScaleSelected)

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

  const commitWidgetUpdate = (widgetId, updates) => {
    if (!resolvedConfig) return
    onConfigChange(updateWidgetInConfig(resolvedConfig, widgetId, updates))
  }

  const commitWidgetUpdates = (updatesById) => {
    if (!resolvedConfig) return
    onConfigChange(updateWidgetsInConfig(resolvedConfig, updatesById))
  }

  useEffect(() => {
    if (!selectedWidgetIds.length || !resolvedConfig) {
      return undefined
    }

    const handleKeyDown = (event) => {
      if (event.key !== 'Delete') {
        return
      }

      if (
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        isEditableElement(event.target)
      ) {
        return
      }

      event.preventDefault()
      onConfigChange(deleteWidgetsInConfig(resolvedConfig, selectedWidgetIds))
      setSelectedWidgetIds([])
      syncPrimarySelectionId(null)
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    onConfigChange,
    resolvedConfig,
    selectedWidgetIds,
    setSelectedWidgetId,
    syncPrimarySelectionId,
  ])

  const { handleSceneMouseDown, handleWidgetMouseDown, handleWheel } =
    useOverlayPointerHandlers({
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

  const handlers = useOverlayMoveableHandlers({
    clearWidgetDraft,
    clearWidgetDrafts,
    commitWidgetUpdate,
    commitWidgetUpdates,
    draftWidgetsRef,
    effectiveSelectedWidgetIds,
    globalScale,
    groupDragSelectionIds,
    interactionStartRef,
    renderedWidgetMap,
    scalePreviewFrameRef,
    selectedTarget,
    selectedWidget,
    selectedWidgets,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setLiveWidgetDraft,
    setLiveWidgetDraftsBatch,
  })

  return {
    activity,
    canResizeSelected,
    canRotateSelected,
    canScaleSelected,
    displayScale,
    elementGuidelines,
    globalOpacity,
    globalScale,
    handleSceneMouseDown,
    handleWheel,
    handleWidgetMouseDown,
    handlers,
    maintainAspectRatio,
    moveableRef,
    previewSecond,
    resolvedConfig,
    sceneElement,
    sceneSize,
    isGroupDragActive,
    selectedTarget,
    selectedTargets,
    selectedWidget,
    selectedWidgetIds,
    selectionRect,
    setSceneElement,
    viewportRef,
    widgetRefCallbacks,
    widgets: renderedWidgets,
  }
}
