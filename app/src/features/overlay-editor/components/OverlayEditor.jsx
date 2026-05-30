/**
 * Main overlay editor component — renders the canvas, Moveable resize handles,
 * widget badge labels, zoom-to-fit viewport, and empty state.
 *
 * Composes focused hooks at the component level instead of relying on a single
 * god hook. Each hook owns one concern and receives only the data it needs.
 */

import { memo, useEffect, useMemo, useRef, useState } from 'react'
import { LayoutGrid, Tag } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import useStore from '@/store/useStore'
import OverlayCanvas from './OverlayCanvas'
import OverlayMoveable from './OverlayMoveable'
import { buildMetricWidgetPreviewModel, buildTextWidgetPreviewModel } from '@/features/widget-preview'
import { WIDGET_ICONS } from '../data/overlayEditorConfig'
import { getWidgetSceneOrigin } from '../utils/overlayEditorHelpers'
import useOverlayEditorState from '../hooks/useOverlayEditorState'
import useWidgetSelection from '../hooks/useWidgetSelection'
import { useEditorViewport } from '../hooks/useEditorViewport'
import { useEditorKeyboard } from '../hooks/useEditorKeyboard'
import useOverlayPointerHandlers from '../utils/createOverlayPointerHandlers'
import { useDragHandlers } from '../hooks/useDragHandlers'
import { useResizeHandlers } from '../hooks/useResizeHandlers'
import { useScaleHandlers } from '../hooks/useScaleHandlers'
import { useRotateHandlers } from '../hooks/useRotateHandlers'

function WidgetBadgeLayer({ activity, displayScale, globalScale, hoveredWidgetId, previewSecond, selectedWidgetIds, widgets }) {
  const visibleWidgets = useMemo(() => {
    const visibleIds = new Set(selectedWidgetIds)
    if (hoveredWidgetId) visibleIds.add(hoveredWidgetId)
    return widgets.filter((widget) => visibleIds.has(widget.id))
  }, [hoveredWidgetId, selectedWidgetIds, widgets])

  if (!visibleWidgets.length) return null

  return (
    <div data-testid="widget-badge-layer" className="pointer-events-none absolute inset-0 z-50 overflow-visible">
      {visibleWidgets.map((widget) => {
        const Icon = WIDGET_ICONS[widget.type] || Tag
        const metricPreviewModel = buildMetricWidgetPreviewModel({ widget, activity, previewSecond })
        const textPreviewModel = buildTextWidgetPreviewModel({ widget })
        const visualBounds = (metricPreviewModel ?? textPreviewModel)?.visualBounds ?? null
        const origin = getWidgetSceneOrigin(widget, null, visualBounds, { boundsScale: widget.category === 'plots' ? 1 : globalScale })
        const left = origin.x * displayScale
        const top = Math.max(origin.y * displayScale - 24, 0)

        return (
          <div
            key={widget.id}
            className="absolute flex h-5 items-center gap-1 rounded-md border border-border/70 bg-card/90 px-2 text-[11px] font-semibold leading-none text-muted-foreground shadow-sm"
            style={{ left, top }}
          >
            <Icon className="h-3 w-3" />
            <span>{widget.type}</span>
          </div>
        )
      })}
    </div>
  )
}

function CanvasStatusBadges({ height, showTemplateStatus, status, width }) {
  return (
    <div data-testid="canvas-status-badges" className="pointer-events-none absolute left-4 top-4 z-50 flex items-center gap-2">
      <div className="rounded-full border border-border/70 bg-card/85 px-3 py-1 text-xs font-medium text-muted-foreground shadow-lg backdrop-blur-sm">
        {width} &times; {height}
      </div>
      {showTemplateStatus ? (
        <Badge
          variant={status === 'Modified' ? 'secondary' : 'outline'}
          className={`h-6 rounded-full text-[10px] shadow-lg backdrop-blur-sm ${status === 'Modified' ? 'border-accent-border bg-surface-accent-soft text-primary' : 'bg-card/85'}`}
        >
          {status}
        </Badge>
      ) : null}
    </div>
  )
}

function EmptyOverlayState() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-xl border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)] backdrop-blur-sm">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">Overlay canvas ready</p>
        <p className="mt-2 text-sm text-muted-foreground">Load a template or add widgets to start positioning the overlay.</p>
      </div>
    </div>
  )
}

function OverlayEditor({
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
  backgroundMode,
  gridVisible,
  snapToGrid,
  showTemplateStatus,
  templateStatus,
}) {
  const [hoveredWidgetId, setHoveredWidgetId] = useState(null)
  const [isGroupDragActive, setIsGroupDragActive] = useState(false)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [selectionRect, setSelectionRect] = useState(null)
  const marqueeCleanupRef = useRef(null)
  const marqueeSelectionRef = useRef(null)

  // Derived state hook — widgets, scene, preview, drafts
  const overlayState = useOverlayEditorState({ config, globalDefaults, onConfigChange, zoomLevel, onZoomLevelChange })

  // Selection management — composed after overlayState so it can consume orderedWidgetIds, renderedWidgetMap, widgetNodes
  const selection = useWidgetSelection({
    orderedWidgetIds: overlayState.orderedWidgetIds,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    widgetNodes: overlayState.widgetNodes,
    isGroupDragActive,
    groupDragSelectionIds,
  })

  // Viewport tracking
  const { viewportRef, fitScale } = useEditorViewport(overlayState.sceneSize)
  const displayScale = fitScale * overlayState.zoomLevel

  // Keyboard shortcuts
  useEditorKeyboard({ config, onConfigChange, selectedWidgetIds: selection.selectedWidgetIds, setWidgetSelection: selection.setWidgetSelection })

  // Pointer handlers
  const { handleSceneMouseDown, handleWidgetMouseDown, handleWheel } = useOverlayPointerHandlers({
    commitSelection: selection.commitSelection,
    displayScale,
    moveableRef: overlayState.moveableRef,
    marqueeCleanupRef,
    marqueeSelectionRef,
    onZoomLevelChange,
    orderedWidgetIds: overlayState.orderedWidgetIds,
    sceneElement: overlayState.sceneElement,
    sceneSize: overlayState.sceneSize,
    selectedWidgetId: selection.selectedWidgetId,
    selectedWidgetIds: selection.selectedWidgetIds,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setSelectionRect,
    setSelectionState: selection.setSelectionState,
    widgetNodes: overlayState.widgetNodes,
  })

  // Moveable interaction hooks
  const activity = useStore.getState().parsedActivity
  const effectiveSelectedWidgetIds = selection.effectiveSelectedWidgetIds

  const dragHandlers = useDragHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    clearWidgetDrafts: overlayState.clearWidgetDrafts,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    commitWidgetUpdates: overlayState.commitWidgetUpdates,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale: overlayState.globalScale,
    groupDragSelectionIds,
    interactionStartRef: overlayState.interactionStartRef,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    previewSecond: overlayState.previewSecond,
    scalePreviewFrameRef: overlayState.scalePreviewFrameRef,
    selectedTarget: selection.selectedTarget,
    selectedWidget: selection.selectedWidget,
    selectedWidgets: selection.selectedWidgets,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
    setLiveWidgetDraftsBatch: overlayState.setLiveWidgetDraftsBatch,
  })

  const resizeHandlers = useResizeHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    previewSecond: overlayState.previewSecond,
    scalePreviewFrameRef: overlayState.scalePreviewFrameRef,
    selectedTarget: selection.selectedTarget,
    selectedWidget: selection.selectedWidget,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
  })

  const scaleHandlers = useScaleHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    previewSecond: overlayState.previewSecond,
    scalePreviewFrameRef: overlayState.scalePreviewFrameRef,
    selectedTarget: selection.selectedTarget,
    selectedWidget: selection.selectedWidget,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
  })

  const rotateHandlers = useRotateHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    activity,
    effectiveSelectedWidgetIds,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    previewSecond: overlayState.previewSecond,
    scalePreviewFrameRef: overlayState.scalePreviewFrameRef,
    selectedTarget: selection.selectedTarget,
    selectedWidget: selection.selectedWidget,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
  })

  const handlers = { ...dragHandlers, ...resizeHandlers, ...scaleHandlers, ...rotateHandlers }

  // Capability flags
  const canResizeSelected = !selection.isGroupSelection && selection.selectedWidget?.category === 'plots'
  const showEdgeResizeHandles = canResizeSelected && selection.selectedWidget?.type === 'elevation'
  const canScaleSelected = Boolean(!selection.isGroupSelection && selection.selectedWidget && selection.selectedWidget.category !== 'plots')
  const canRotateSelected = !selection.isGroupSelection && selection.selectedWidget?.type === 'course'
  const maintainAspectRatio = !selection.isGroupSelection && (selection.selectedWidget?.type === 'course' || canScaleSelected)

  // Marquee cleanup
  useEffect(
    () => () => {
      marqueeCleanupRef.current?.()
    },
    [],
  )

  const valueFont =
    config?.values?.find((v) => v.font || v.font_family)?.font ||
    config?.values?.find((v) => v.font || v.font_family)?.font_family ||
    globalDefaults?.font_values

  const canvasSceneProps = useMemo(
    () => ({
      sceneFont: config?.scene?.font,
      sceneFontSize: config?.scene?.font_size,
      sceneStyle: overlayState.sceneStyle,
      valueFont,
      sceneSize: overlayState.sceneSize,
    }),
    [config?.scene?.font, config?.scene?.font_size, overlayState.sceneStyle, valueFont, overlayState.sceneSize],
  )
  const canvasDisplayProps = useMemo(
    () => ({ displayScale, globalScale: overlayState.globalScale, globalOpacity: overlayState.globalOpacity, backgroundMode, gridVisible }),
    [displayScale, overlayState.globalScale, overlayState.globalOpacity, backgroundMode, gridVisible],
  )
  const canvasDataProps = useMemo(
    () => ({
      widgets: overlayState.renderedWidgets,
      activity,
      previewSecond: overlayState.previewSecond,
      selectionRect,
      exportRange: overlayState.previewExportRange,
    }),
    [overlayState.renderedWidgets, activity, overlayState.previewSecond, selectionRect, overlayState.previewExportRange],
  )
  const canvasCallbacks = useMemo(
    () => ({
      setSceneElement: overlayState.setSceneElement,
      handleSceneMouseDown,
      handleWidgetMouseDown,
      setHoveredWidgetId,
      widgetRefCallbacks: overlayState.widgetRefCallbacks,
    }),
    [overlayState.setSceneElement, handleSceneMouseDown, handleWidgetMouseDown, overlayState.widgetRefCallbacks],
  )

  if (!config) return <EmptyOverlayState />

  return (
    <div ref={viewportRef} className="relative flex h-full flex-1 overflow-hidden" onWheel={handleWheel}>
      <div className="relative flex h-full w-full items-center justify-center overflow-hidden p-8">
        <CanvasStatusBadges
          height={overlayState.sceneSize.height}
          showTemplateStatus={showTemplateStatus}
          status={templateStatus}
          width={overlayState.sceneSize.width}
        />
        <div
          className="relative shrink-0"
          style={{ width: overlayState.sceneSize.width * displayScale, height: overlayState.sceneSize.height * displayScale }}
        >
          <div
            className="absolute left-0 top-0"
            style={{
              width: overlayState.sceneSize.width,
              height: overlayState.sceneSize.height,
              transform: `scale(${displayScale})`,
              transformOrigin: 'top left',
            }}
          >
            <OverlayCanvas sceneProps={canvasSceneProps} displayProps={canvasDisplayProps} dataProps={canvasDataProps} callbacks={canvasCallbacks} />
            <OverlayMoveable
              moveableRef={overlayState.moveableRef}
              selectedTarget={selection.selectedTarget}
              selectedTargets={selection.selectedTargets}
              isGroupDragActive={isGroupDragActive}
              sceneElement={overlayState.sceneElement}
              displayScale={displayScale}
              canResizeSelected={canResizeSelected}
              canScaleSelected={canScaleSelected}
              canRotateSelected={canRotateSelected}
              maintainAspectRatio={maintainAspectRatio}
              showEdgeResizeHandles={showEdgeResizeHandles}
              elementGuidelines={selection.elementGuidelines}
              sceneSize={overlayState.sceneSize}
              snapToGrid={snapToGrid}
              handlers={handlers}
            />
          </div>
          <WidgetBadgeLayer
            activity={activity}
            displayScale={displayScale}
            globalScale={overlayState.globalScale}
            hoveredWidgetId={hoveredWidgetId}
            previewSecond={overlayState.previewSecond}
            selectedWidgetIds={selection.selectedWidgetIds}
            widgets={overlayState.renderedWidgets}
          />
        </div>
      </div>
    </div>
  )
}

export default memo(OverlayEditor)
