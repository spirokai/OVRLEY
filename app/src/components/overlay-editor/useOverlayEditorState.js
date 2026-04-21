import { useEffect, useMemo, useRef, useState } from 'react'
import { getCurrentParsedActivity } from '../../api/activityCache'
import useStore from '../../store/useStore'
import {
  buildConfigWidgets,
  deleteWidgetInConfig,
  updateWidgetInConfig,
} from '@/lib/widget-config'
import { applyGlobalDefaults } from '@/lib/config-utils'
import { DEFAULT_GRADIENT_TRIANGLE_WIDTH } from './constants'
import { buildWidgetTransform, clamp, getSceneSize } from './utils'

function clearLiveWidgetDraft(draftWidgetsRef, widgetId) {
  delete draftWidgetsRef.current[widgetId]
}

function isEditableElement(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(
    target.closest('input, textarea, select, [contenteditable="true"]'),
  )
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
  const [liveWidgetDrafts, setLiveWidgetDrafts] = useState({})
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })
  const [sceneElement, setSceneElement] = useState(null)
  const [widgetNodes, setWidgetNodes] = useState({})

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
  }, [resolvedConfig])

  const setLiveWidgetDraft = (widgetId, nextDraft) => {
    draftWidgetsRef.current[widgetId] = nextDraft
    setLiveWidgetDrafts((current) => ({
      ...current,
      [widgetId]: nextDraft,
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
  const selectedWidget = useMemo(
    () =>
      renderedWidgets.find((widget) => widget.id === selectedWidgetId) || null,
    [renderedWidgets, selectedWidgetId],
  )
  const selectedWidgetDataSignature = useMemo(
    () => JSON.stringify(selectedWidget?.data ?? null),
    [selectedWidget],
  )
  const selectedTarget = selectedWidgetId
    ? widgetNodes[selectedWidgetId] || null
    : null
  const elementGuidelines = useMemo(
    () =>
      widgets
        .filter((widget) => widget.id !== selectedWidgetId)
        .map((widget) => widgetNodes[widget.id])
        .filter(Boolean),
    [selectedWidgetId, widgetNodes, widgets],
  )

  useEffect(() => {
    if (!moveableRef.current || !selectedTarget) return undefined

    const frameId = requestAnimationFrame(() => {
      moveableRef.current?.updateRect()
    })

    return () => cancelAnimationFrame(frameId)
  }, [
    selectedTarget,
    selectedWidgetId,
    selectedWidgetDataSignature,
    globalScale,
    displayScale,
  ])

  const canResizeSelected = selectedWidget?.category === 'plots'
  const canScaleSelected = Boolean(
    selectedWidget && selectedWidget.category !== 'plots',
  )
  const canRotateSelected = selectedWidget?.type === 'course'
  const maintainAspectRatio =
    selectedWidget?.type === 'course' || canScaleSelected

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

  const handleWheel = (event) => {
    event.preventDefault()
    const delta = event.deltaY < 0 ? 0.1 : -0.1
    onZoomLevelChange((current) =>
      clamp(Number((current + delta).toFixed(2)), 0.35, 4),
    )
  }

  useEffect(() => {
    if (!selectedWidgetId || !resolvedConfig) {
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
      onConfigChange(deleteWidgetInConfig(resolvedConfig, selectedWidgetId))
      setSelectedWidgetId(null)
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onConfigChange, resolvedConfig, selectedWidgetId, setSelectedWidgetId])

  const handlers = {
    onDragStart: () => {
      if (!selectedWidget) return

      interactionStartRef.current = {
        id: selectedWidget.id,
        x: selectedWidget.data.x ?? 0,
        y: selectedWidget.data.y ?? 0,
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
    handleWheel,
    handlers,
    maintainAspectRatio,
    moveableRef,
    resolvedConfig,
    previewSecond,
    sceneElement,
    sceneSize,
    selectedTarget,
    setSceneElement,
    setSelectedWidgetId,
    selectedWidget,
    viewportRef,
    widgetRefCallbacks,
    widgets: renderedWidgets,
  }
}
