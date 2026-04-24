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
import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import { buildWidgetTransform, clamp, getSceneSize } from './utils'

function clearLiveWidgetDraft(draftWidgetsRef, widgetId) {
  delete draftWidgetsRef.current[widgetId]
}

function clearLiveWidgetDrafts(draftWidgetsRef, widgetIds) {
  widgetIds.forEach((widgetId) => {
    delete draftWidgetsRef.current[widgetId]
  })
}

function isEditableElement(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(
    target.closest('input, textarea, select, [contenteditable="true"]'),
  )
}

function hasSelectionModifier(event) {
  return event.metaKey || event.ctrlKey || event.shiftKey
}

function buildSelectionRect(start, end) {
  const left = Math.min(start.x, end.x)
  const top = Math.min(start.y, end.y)
  const right = Math.max(start.x, end.x)
  const bottom = Math.max(start.y, end.y)

  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  }
}

function rectanglesIntersect(firstRect, secondRect) {
  return !(
    firstRect.x + firstRect.width < secondRect.x ||
    secondRect.x + secondRect.width < firstRect.x ||
    firstRect.y + firstRect.height < secondRect.y ||
    secondRect.y + secondRect.height < firstRect.y
  )
}

function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set(widgetIds.filter(Boolean))
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}

function getWidgetIdFromTarget(target) {
  if (!(target instanceof HTMLElement)) {
    return null
  }

  return target.dataset.widgetId || null
}

function applyLiveWidgetStyles(target, widget, draft, globalScale) {
  if (!target || !widget) {
    return
  }

  const nextX = draft.x ?? widget.data.x ?? 0
  const nextY = draft.y ?? widget.data.y ?? 0
  const nextWidth = draft.width ?? widget.data.width
  const nextHeight = draft.height ?? widget.data.height
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const nextScale = (draft.scale ?? 1) * globalScale

  target.style.left = `${nextX}px`
  target.style.top = `${nextY}px`

  if (typeof nextWidth === 'number') {
    target.style.width = `${nextWidth}px`
  }

  if (typeof nextHeight === 'number') {
    target.style.height = `${nextHeight}px`
  }

  target.style.transform =
    buildWidgetTransform({
      rotation: nextRotation,
      scale: nextScale,
    }) || ''
}

function applyLiveScalePositionStyles(target, widget, draft, globalScale) {
  if (!target || !widget) {
    return
  }

  const baseX = widget.data.x ?? 0
  const baseY = widget.data.y ?? 0
  const nextTranslateX = (draft.x ?? baseX) - baseX
  const nextTranslateY = (draft.y ?? baseY) - baseY
  const nextRotation =
    draft.rotation ??
    (widget.type === 'course' ? (widget.data.rotation ?? 0) : 0)
  const transforms = [`translate(${nextTranslateX}px, ${nextTranslateY}px)`]

  if (nextRotation) {
    transforms.push(`rotate(${nextRotation}deg)`)
  }

  if (globalScale !== 1) {
    transforms.push(`scale(${globalScale})`)
  }

  target.style.left = `${baseX}px`
  target.style.top = `${baseY}px`
  target.style.transform = transforms.join(' ')
}

function buildScaledWidgetDataDraft(origin, scaleFactor) {
  const nextFontSize = clamp(
    Math.round((origin.fontSize || 60) * scaleFactor),
    8,
    400,
  )
  const nextIconSize = clamp(
    Math.round((origin.iconSize || 28) * scaleFactor),
    0,
    400,
  )
  const nextIconOffsetX = Math.round((origin.iconOffsetX || 0) * scaleFactor)
  const nextIconOffsetY = Math.round((origin.iconOffsetY || 0) * scaleFactor)
  const nextTriangleWidth = clamp(
    Math.round(origin.triangleWidth * scaleFactor),
    0,
    600,
  )
  const nextValueOffset = Math.round((origin.valueOffset || 0) * scaleFactor)

  return {
    font_size: nextFontSize,
    icon_size: nextIconSize,
    icon_offset_x: nextIconOffsetX,
    icon_offset_y: nextIconOffsetY,
    triangle_width: nextTriangleWidth,
    value_offset: nextValueOffset,
  }
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
  const viewportRef = useRef(null)
  const moveableRef = useRef(null)
  const interactionStartRef = useRef(null)
  const draftWidgetsRef = useRef({})
  const scalePreviewFrameRef = useRef(null)
  const selectionSyncRef = useRef(false)
  const marqueeCleanupRef = useRef(null)
  const marqueeSelectionRef = useRef(null)
  const [liveWidgetDrafts, setLiveWidgetDrafts] = useState({})
  const [isGroupDragActive, setIsGroupDragActive] = useState(false)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })
  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})
  const [selectedWidgetIds, setSelectedWidgetIds] = useState([])
  const [selectionRect, setSelectionRect] = useState(null)

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
    const rawSecond = Number(selectedSecond) || 0
    const startSecond = Number(resolvedConfig?.scene?.start) || 0
    const activityEndSecond = Number(
      sourceActivity?.trim_end_seconds ??
        sourceActivity?.metadata?.duration_seconds ??
        startSecond,
    )
    const sceneEndSecond = Number(resolvedConfig?.scene?.end)
    const maxSecond = Number.isFinite(sceneEndSecond)
      ? Math.min(sceneEndSecond, activityEndSecond)
      : activityEndSecond

    return clamp(rawSecond, startSecond, Math.max(maxSecond, startSecond))
  }, [
    resolvedConfig?.scene?.end,
    resolvedConfig?.scene?.start,
    selectedSecond,
    sourceActivity?.metadata?.duration_seconds,
    sourceActivity?.trim_end_seconds,
  ])

  useEffect(() => {
    draftWidgetsRef.current = {}
    setLiveWidgetDrafts({})
    setIsGroupDragActive(false)
    setGroupDragSelectionIds([])
  }, [resolvedConfig])

  const setLiveWidgetDraft = (widgetId, nextDraft) => {
    draftWidgetsRef.current[widgetId] = nextDraft
    setLiveWidgetDrafts((current) => ({
      ...current,
      [widgetId]: nextDraft,
    }))
  }

  const setLiveWidgetDraftsBatch = (nextDraftsById) => {
    Object.entries(nextDraftsById).forEach(([widgetId, nextDraft]) => {
      draftWidgetsRef.current[widgetId] = nextDraft
    })

    setLiveWidgetDrafts((current) => ({
      ...current,
      ...nextDraftsById,
    }))
  }

  const clearWidgetDraft = (widgetId) => {
    clearLiveWidgetDraft(draftWidgetsRef, widgetId)
    setLiveWidgetDrafts((current) => {
      if (!current[widgetId]) {
        return current
      }

      const next = { ...current }
      delete next[widgetId]
      return next
    })
  }

  const clearWidgetDrafts = (widgetIds) => {
    clearLiveWidgetDrafts(draftWidgetsRef, widgetIds)
    setLiveWidgetDrafts((current) => {
      const next = { ...current }
      let changed = false

      widgetIds.forEach((widgetId) => {
        if (!next[widgetId]) {
          return
        }

        delete next[widgetId]
        changed = true
      })

      return changed ? next : current
    })
  }

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
    () =>
      widgets.map((widget) => {
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
      }),
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
    const selectionChanged =
      normalizedIds.length !== selectedWidgetIds.length ||
      normalizedIds.some(
        (widgetId, index) => widgetId !== selectedWidgetIds[index],
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

  const handleWheel = (event) => {
    event.preventDefault()
    const delta = event.deltaY < 0 ? 0.1 : -0.1
    onZoomLevelChange((current) =>
      clamp(Number((current + delta).toFixed(2)), 0.35, 4),
    )
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

  const getScenePoint = (clientX, clientY) => {
    if (!sceneElement) {
      return null
    }

    const sceneBounds = sceneElement.getBoundingClientRect()
    return {
      x: clamp((clientX - sceneBounds.left) / displayScale, 0, sceneSize.width),
      y: clamp((clientY - sceneBounds.top) / displayScale, 0, sceneSize.height),
    }
  }

  const getIntersectedWidgetIds = (nextSelectionRect) => {
    if (!sceneElement) {
      return []
    }

    const sceneBounds = sceneElement.getBoundingClientRect()

    return orderedWidgetIds.filter((widgetId) => {
      const node = widgetNodes[widgetId]
      if (!node) {
        return false
      }

      const nodeBounds = node.getBoundingClientRect()
      const nodeRect = {
        x: (nodeBounds.left - sceneBounds.left) / displayScale,
        y: (nodeBounds.top - sceneBounds.top) / displayScale,
        width: nodeBounds.width / displayScale,
        height: nodeBounds.height / displayScale,
      }

      return rectanglesIntersect(nextSelectionRect, nodeRect)
    })
  }

  const handleWidgetMouseDown = (event, widgetId) => {
    event.stopPropagation()

    if (event.button !== 0) {
      return
    }

    const isSelected = selectedWidgetIds.includes(widgetId)
    if (hasSelectionModifier(event)) {
      const nextIds = isSelected
        ? selectedWidgetIds.filter((selectedId) => selectedId !== widgetId)
        : [...selectedWidgetIds, widgetId]
      const nextPrimaryId = isSelected
        ? getPrimarySelectionId(
            nextIds,
            selectedWidgetId === widgetId ? null : selectedWidgetId,
          )
        : widgetId

      commitSelection(nextIds, nextPrimaryId)
      return
    }

    if (isSelected && selectedWidgetIds.length > 1) {
      event.preventDefault()
      const draggedWidgetIds = [...selectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      moveableRef.current?.dragStart(event.nativeEvent, event.currentTarget)
      return
    }

    if (selectedWidgetIds.length === 1 && selectedWidgetId === widgetId) {
      return
    }

    commitSelection([widgetId], widgetId)
  }

  const handleSceneMouseDown = (event) => {
    if (event.button !== 0) {
      return
    }

    const startPoint = getScenePoint(event.clientX, event.clientY)
    if (!startPoint) {
      return
    }

    event.preventDefault()
    marqueeCleanupRef.current?.()

    const additive = hasSelectionModifier(event)
    const baseIds = additive ? selectedWidgetIds : []
    marqueeSelectionRef.current = {
      additive,
      baseIds,
      hasMoved: false,
      previewIds: baseIds,
      startPoint,
    }

    setSelectionRect({
      x: startPoint.x,
      y: startPoint.y,
      width: 0,
      height: 0,
    })

    const handleWindowMouseMove = (moveEvent) => {
      const nextPoint = getScenePoint(moveEvent.clientX, moveEvent.clientY)
      const gesture = marqueeSelectionRef.current
      if (!nextPoint || !gesture) {
        return
      }

      const nextRect = buildSelectionRect(gesture.startPoint, nextPoint)
      const hasMoved = nextRect.width > 2 || nextRect.height > 2
      const hitIds = hasMoved ? getIntersectedWidgetIds(nextRect) : []
      const nextIds = additive
        ? normalizeSelectionIds([...baseIds, ...hitIds], orderedWidgetIds)
        : hitIds

      marqueeSelectionRef.current = {
        ...gesture,
        hasMoved,
        previewIds: nextIds,
      }

      setSelectionRect(nextRect)
      if (hasMoved) {
        setSelectionState(nextIds)
      }
    }

    const handleWindowMouseUp = () => {
      const gesture = marqueeSelectionRef.current

      marqueeCleanupRef.current?.()
      marqueeSelectionRef.current = null
      setSelectionRect(null)

      if (!gesture) {
        return
      }

      if (!gesture.hasMoved) {
        commitSelection(gesture.baseIds, selectedWidgetId)
        return
      }

      commitSelection(
        gesture.previewIds,
        getPrimarySelectionId(gesture.previewIds, selectedWidgetId),
      )
    }

    marqueeCleanupRef.current = () => {
      window.removeEventListener('mousemove', handleWindowMouseMove)
      window.removeEventListener('mouseup', handleWindowMouseUp)
      marqueeCleanupRef.current = null
    }

    window.addEventListener('mousemove', handleWindowMouseMove)
    window.addEventListener('mouseup', handleWindowMouseUp)
  }

  const handlers = {
    onDragStart: () => {
      if (!selectedWidget) return

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        type: 'single-drag',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onDrag: ({ beforeTranslate, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: origin.x + beforeTranslate[0],
        y: origin.y + beforeTranslate[1],
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(target, selectedWidget, nextDraft, globalScale)
    },
    onDragEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onDragGroupStart: () => {
      if (!selectedWidgets.length) return

      const draggedWidgetIds = [...effectiveSelectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      interactionStartRef.current = {
        type: 'group-drag',
        widgetIds: draggedWidgetIds,
        widgetsById: Object.fromEntries(
          selectedWidgets.map((widget) => [
            widget.id,
            {
              x: widget.data.x ?? 0,
              y: widget.data.y ?? 0,
            },
          ]),
        ),
      }

      draggedWidgetIds.forEach((widgetId) => {
        draftWidgetsRef.current[widgetId] = {}
      })
    },
    onDragGroup: ({ events }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const nextDraftsById = {}

      events.forEach((childEvent) => {
        const widgetId = getWidgetIdFromTarget(childEvent.target)
        const widget = widgetId ? renderedWidgetMap[widgetId] : null
        const widgetOrigin = widgetId ? origin.widgetsById[widgetId] : null

        if (!widgetId || !widget || !widgetOrigin) {
          return
        }

        const nextDraft = {
          ...draftWidgetsRef.current[widgetId],
          x: widgetOrigin.x + childEvent.beforeTranslate[0],
          y: widgetOrigin.y + childEvent.beforeTranslate[1],
        }

        nextDraftsById[widgetId] = nextDraft
        applyLiveWidgetStyles(childEvent.target, widget, nextDraft, globalScale)
      })

      if (Object.keys(nextDraftsById).length) {
        setLiveWidgetDraftsBatch(nextDraftsById)
      }
    },
    onDragGroupEnd: () => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'group-drag') return

      const draggedWidgetIds = origin.widgetIds?.length
        ? [...origin.widgetIds]
        : [...groupDragSelectionIds]
      const updatesById = draggedWidgetIds.reduce((accumulator, widgetId) => {
        const draft = draftWidgetsRef.current[widgetId]
        const widgetOrigin = origin.widgetsById[widgetId]

        if (!draft || !widgetOrigin) {
          return accumulator
        }

        accumulator[widgetId] = {
          x: Math.round(draft.x ?? widgetOrigin.x),
          y: Math.round(draft.y ?? widgetOrigin.y),
        }
        return accumulator
      }, {})

      if (Object.keys(updatesById).length) {
        commitWidgetUpdates(updatesById)
      }

      clearWidgetDrafts(draggedWidgetIds)
      setIsGroupDragActive(false)
      setGroupDragSelectionIds([])
      interactionStartRef.current = null
    },
    onResizeStart: ({ dragStart }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        width: selectedWidget.data.width ?? 0,
        height: selectedWidget.data.height ?? 0,
        markerSize: selectedWidget.data.marker_size ?? null,
        type: 'resize',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onResize: ({ width, height, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextX = origin.x + drag.beforeTranslate[0]
      const nextY = origin.y + drag.beforeTranslate[1]
      const nextWidth = Math.max(width, 8)
      const nextHeight = Math.max(height, 8)
      const widthScale = origin.width ? nextWidth / origin.width : 1
      const heightScale = origin.height ? nextHeight / origin.height : 1
      const markerScale = (widthScale + heightScale) / 2
      const nextMarkerSize =
        origin.markerSize === null
          ? undefined
          : clamp(Math.round(origin.markerSize * markerScale), 0, 400)

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: nextX,
        y: nextY,
        width: nextWidth,
        height: nextHeight,
        ...(nextMarkerSize === undefined
          ? {}
          : { marker_size: nextMarkerSize }),
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(
        target ?? drag.target,
        selectedWidget,
        nextDraft,
        globalScale,
      )
    },
    onResizeEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          width: Math.max(Math.round(draft.width ?? 0), 0),
          height: Math.max(Math.round(draft.height ?? 0), 0),
          ...(draft.marker_size === undefined
            ? {}
            : { marker_size: Math.max(Math.round(draft.marker_size), 0) }),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onScaleStart: ({ dragStart, target }) => {
      if (!selectedWidget) return

      if (dragStart) {
        dragStart.set([0, 0])
      }

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        fontSize: selectedWidget.data.font_size ?? 60,
        iconSize: selectedWidget.data.icon_size ?? 28,
        iconOffsetX: selectedWidget.data.icon_offset_x ?? 0,
        iconOffsetY: selectedWidget.data.icon_offset_y ?? 0,
        triangleWidth:
          selectedWidget.data.triangle_width ?? DEFAULT_GRADIENT_TRIANGLE_WIDTH,
        valueOffset: selectedWidget.data.value_offset ?? 0,
        renderedWidth: target?.offsetWidth ?? selectedTarget?.offsetWidth ?? 0,
        renderedHeight:
          target?.offsetHeight ?? selectedTarget?.offsetHeight ?? 0,
        type: 'scale',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onScale: ({ scale, direction, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return
      const uniformScale = Number.isFinite(scale?.[0])
        ? scale[0]
        : Number.isFinite(scale?.[1])
          ? scale[1]
          : 1

      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        scale_direction: direction,
        ...buildScaledWidgetDataDraft(origin, uniformScale),
      }

      draftWidgetsRef.current[origin.id] = nextDraft
      setLiveWidgetDraft(
        origin.id,
        buildScaledWidgetDataDraft(origin, uniformScale),
      )

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
      }

      scalePreviewFrameRef.current = requestAnimationFrame(() => {
        const targetNode = target ?? selectedTarget
        if (!targetNode) return

        const measuredWidth = targetNode.offsetWidth
        const measuredHeight = targetNode.offsetHeight
        const measuredDraft = {
          ...draftWidgetsRef.current[origin.id],
          x:
            origin.x +
            (direction?.[0] === -1 ? origin.renderedWidth - measuredWidth : 0),
          y:
            origin.y +
            (direction?.[1] === -1
              ? origin.renderedHeight - measuredHeight
              : 0),
        }

        draftWidgetsRef.current[origin.id] = measuredDraft
        applyLiveScalePositionStyles(
          targetNode,
          selectedWidget,
          measuredDraft,
          globalScale,
        )
      })
    },
    onScaleEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      if (scalePreviewFrameRef.current) {
        cancelAnimationFrame(scalePreviewFrameRef.current)
        scalePreviewFrameRef.current = null
      }

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        const targetNode = selectedTarget
        const measuredWidth =
          targetNode?.offsetWidth ?? origin.renderedWidth ?? 0
        const measuredHeight =
          targetNode?.offsetHeight ?? origin.renderedHeight ?? 0
        const finalDirection = Array.isArray(draft.scale_direction)
          ? draft.scale_direction
          : [1, 1]
        const finalX =
          origin.x +
          (finalDirection[0] === -1 ? origin.renderedWidth - measuredWidth : 0)
        const finalY =
          origin.y +
          (finalDirection[1] === -1
            ? origin.renderedHeight - measuredHeight
            : 0)

        commitWidgetUpdate(origin.id, {
          x: Math.round(finalX),
          y: Math.round(finalY),
          font_size: draft.font_size ?? origin.fontSize ?? 60,
          icon_size: draft.icon_size ?? origin.iconSize ?? 28,
          icon_offset_x: draft.icon_offset_x ?? origin.iconOffsetX ?? 0,
          icon_offset_y: draft.icon_offset_y ?? origin.iconOffsetY ?? 0,
          triangle_width: draft.triangle_width ?? origin.triangleWidth ?? 0,
          value_offset: draft.value_offset ?? origin.valueOffset ?? 0,
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
    onRotateStart: () => {
      if (!selectedWidget) return

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
        rotation: selectedWidget.data.rotation ?? 0,
        type: 'rotate',
      }
      draftWidgetsRef.current[selectedWidget.id] = {}
    },
    onRotate: ({ beforeRotate, drag, target }) => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const nextX = origin.x + (drag?.beforeTranslate?.[0] ?? 0)
      const nextY = origin.y + (drag?.beforeTranslate?.[1] ?? 0)
      const nextRotation = beforeRotate
      const nextDraft = {
        ...draftWidgetsRef.current[origin.id],
        x: nextX,
        y: nextY,
        rotation: nextRotation,
      }

      setLiveWidgetDraft(origin.id, nextDraft)
      applyLiveWidgetStyles(target, selectedWidget, nextDraft, globalScale)
    },
    onRotateEnd: () => {
      const origin = interactionStartRef.current
      if (!origin?.id) return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft) {
        const normalizedRotation =
          (((draft.rotation ?? origin.rotation ?? 0) % 360) + 360) % 360

        commitWidgetUpdate(origin.id, {
          x: Math.round(draft.x ?? origin.x),
          y: Math.round(draft.y ?? origin.y),
          rotation: Number(normalizedRotation.toFixed(1)),
        })
      }

      clearWidgetDraft(origin.id)
      interactionStartRef.current = null
    },
  }

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
