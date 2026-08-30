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
import EditorToolbar from '@/features/app-shell/components/EditorToolbar'
import OverlayCanvas from './OverlayCanvas'
import OverlayMoveable from './OverlayMoveable'
import { WIDGET_ICONS } from '../data/overlayEditorConfig'
import { useOverlayEditorStateWithLiveEdits } from '../hooks/useOverlayEditorState'
import useOverlayPreviewModels from '../hooks/useOverlayPreviewModels'
import useWidgetSelection from '../hooks/useWidgetSelection'
import { useEditorViewport } from '../hooks/useEditorViewport'
import { useEditorKeyboard } from '../hooks/useEditorKeyboard'
import useOverlayPointerHandlers from '../utils/createOverlayPointerHandlers'
import { useDragHandlers } from '../hooks/useDragHandlers'
import { useResizeHandlers } from '../hooks/useResizeHandlers'
import { useScaleHandlers } from '../hooks/useScaleHandlers'
import { useRotateHandlers } from '../hooks/useRotateHandlers'
import { isBackdropWidget, isFramedWidget } from '@/lib/widget/display-type-behavior'
import { buildRenderedGeometrySignature, buildWidgetRenderGeometryModels } from '../utils/widgetRenderGeometry'
import { isUniformResizeDisplayType } from '../utils/widgetResizeScaling'

function WidgetBadgeLayer({ displayScale, hoveredWidgetId, renderGeometryModels, selectedWidgetIds, widgets }) {
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
        const renderGeometryModel = renderGeometryModels[widget.id]
        const { renderGeometry } = renderGeometryModel
        const left = renderGeometry.badgeLeft * displayScale - 2
        const top = Math.max(renderGeometry.badgeTop * displayScale - 20, 0)

        return (
          <div
            key={widget.id}
            className="ml-1 absolute flex h-5 items-center gap-1 rounded-xs border border-border/70 bg-card/90 px-2 text-[11px] font-semibold leading-none text-muted-foreground shadow-sm"
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

function CanvasStatusBadges({ height, showProjectStatus, status, width }) {
  return (
    <div data-testid="canvas-status-badges" className="pointer-events-none absolute right-8 top-4 z-50 flex items-center gap-2">
      {showProjectStatus ? (
        <Badge
          variant={status === 'Modified' ? 'secondary' : 'outline'}
          className={`h-6 rounded-xs text-[10px] shadow-lg backdrop-blur-sm ${status === 'Modified' ? 'border-accent-border bg-surface-accent-soft/20 text-primary' : 'bg-card/85'}`}
        >
          {status}
        </Badge>
      ) : null}
      <div className="rounded-xs border border-border/70 bg-card/85 px-3 py-1 text-xs font-medium text-muted-foreground shadow-lg backdrop-blur-sm">
        {width} &times; {height}
      </div>
    </div>
  )
}

function CanvasToolbar({ editorShell, importedBackgroundImageFilename, importedVideoFilename, undoRedoControls }) {
  return (
    <div data-testid="canvas-editor-toolbar" className="pointer-events-auto absolute left-1/2 top-4 z-50 -translate-x-1/2">
      <EditorToolbar
        backgroundMode={editorShell.editorBackgroundMode}
        onSetBackgroundMode={editorShell.setEditorBackgroundMode}
        importedBackgroundImageFilename={importedBackgroundImageFilename}
        importedVideoFilename={importedVideoFilename}
        zoomLevel={editorShell.editorZoomLevel}
        onZoomIn={editorShell.increaseZoom}
        onZoomOut={editorShell.decreaseZoom}
        onResetZoom={editorShell.resetZoom}
        gridVisible={editorShell.editorGridVisible}
        onSetGridVisible={editorShell.setEditorGridVisible}
        snapToGrid={editorShell.editorSnapToGrid}
        onSetSnapToGrid={editorShell.setEditorSnapToGrid}
        undoRedoControls={undoRedoControls}
      />
    </div>
  )
}

function EmptyOverlayState() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-sm border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)] backdrop-blur-sm">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">Overlay canvas ready</p>
        <p className="mt-2 text-sm text-muted-foreground">Load a template or add widgets to start positioning the overlay.</p>
      </div>
    </div>
  )
}

function OverlayEditorContent({
  editorShell,
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
  backgroundMode,
  gridVisible,
  snapToGrid,
  importedBackgroundImageFilename,
  importedVideoFilename,
  showProjectStatus,
  projectStatus,
  undoRedoControls,
  widgetLiveEdits,
}) {
  const [hoveredWidgetId, setHoveredWidgetId] = useState(null)
  const [isGroupDragActive, setIsGroupDragActive] = useState(false)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [selectionRect, setSelectionRect] = useState(null)
  const [stageElement, setStageElement] = useState(null)
  const clipboardRef = useRef(null)
  const marqueeCleanupRef = useRef(null)
  const marqueeSelectionRef = useRef(null)

  // Derived state hook — widgets, scene, preview, drafts
  const overlayState = useOverlayEditorStateWithLiveEdits({ config, globalDefaults, onConfigChange }, widgetLiveEdits)
  const activity = overlayState.activity
  const { metricPreviewModels, textPreviewModels } = useOverlayPreviewModels({
    activity,
    previewSecond: overlayState.previewSecond,
    renderedWidgets: overlayState.canvasWidgets,
  })

  const renderGeometryModels = useMemo(
    () =>
      buildWidgetRenderGeometryModels({
        widgets: overlayState.canvasWidgets,
        metricPreviewModels,
        textPreviewModels,
        globalScale: overlayState.globalScale,
        liveWidgetDrafts: overlayState.liveWidgetDrafts,
      }),
    [metricPreviewModels, overlayState.canvasWidgets, overlayState.globalScale, overlayState.liveWidgetDrafts, textPreviewModels],
  )

  // Selection management — composed after overlayState so it can consume orderedWidgetIds, renderedWidgetMap, widgetNodes
  const selection = useWidgetSelection({
    orderedWidgetIds: overlayState.orderedWidgetIds,
    renderedWidgetMap: overlayState.renderedWidgetMap,
    widgetNodes: overlayState.widgetNodes,
    isGroupDragActive,
    groupDragSelectionIds,
  })

  // Viewport tracking
  const { displayScale, handleWheel, scrollViewportRef, viewportRef } = useEditorViewport({
    onZoomLevelChange,
    sceneElement: overlayState.sceneElement,
    sceneSize: overlayState.sceneSize,
    zoomLevel,
  })

  // Keyboard shortcuts
  useEditorKeyboard({
    config,
    editorShell,
    onConfigChange,
    selectedWidgetIds: selection.selectedWidgetIds,
    selectedWidgets: selection.selectedWidgets,
    setWidgetSelection: selection.setWidgetSelection,
    clipboardRef,
  })

  // Pointer handlers
  const { handleSceneMouseDown, handleWidgetMouseDown } = useOverlayPointerHandlers({
    commitSelection: selection.commitSelection,
    moveableRef: overlayState.moveableRef,
    marqueeCleanupRef,
    marqueeSelectionRef,
    orderedWidgetIds: overlayState.orderedWidgetIds,
    sceneElement: overlayState.sceneElement,
    sceneSize: overlayState.sceneSize,
    stageElement,
    selectedWidgetId: selection.selectedWidgetId,
    selectedWidgetIds: selection.selectedWidgetIds,
    setGroupDragSelectionIds,
    setIsGroupDragActive,
    setSelectionRect,
    setSelectionState: selection.setSelectionState,
    widgetNodes: overlayState.widgetNodes,
  })

  // Moveable interaction hooks
  const effectiveSelectedWidgetIds = selection.effectiveSelectedWidgetIds

  const dragHandlers = useDragHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    clearWidgetDrafts: overlayState.clearWidgetDrafts,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    commitWidgetUpdates: overlayState.commitWidgetUpdates,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    beginWidgetInteraction: overlayState.beginWidgetInteraction,
    endWidgetInteraction: overlayState.endWidgetInteraction,
    effectiveSelectedWidgetIds,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
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
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    selectedWidget: selection.selectedWidget,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
    beginWidgetInteraction: overlayState.beginWidgetInteraction,
    endWidgetInteraction: overlayState.endWidgetInteraction,
  })

  const scaleHandlers = useScaleHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    clearWidgetDrafts: overlayState.clearWidgetDrafts,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    commitWidgetUpdates: overlayState.commitWidgetUpdates,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    selectedTarget: selection.selectedTarget,
    selectedWidget: selection.selectedWidget,
    selectedWidgets: selection.selectedWidgets,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
    setLiveWidgetDraftsBatch: overlayState.setLiveWidgetDraftsBatch,
    beginWidgetInteraction: overlayState.beginWidgetInteraction,
    endWidgetInteraction: overlayState.endWidgetInteraction,
  })

  const rotateHandlers = useRotateHandlers({
    clearWidgetDraft: overlayState.clearWidgetDraft,
    commitWidgetUpdate: overlayState.commitWidgetUpdate,
    draftWidgetsRef: overlayState.draftWidgetsRef,
    globalScale: overlayState.globalScale,
    interactionStartRef: overlayState.interactionStartRef,
    selectedWidget: selection.selectedWidget,
    setLiveWidgetDraft: overlayState.setLiveWidgetDraft,
    beginWidgetInteraction: overlayState.beginWidgetInteraction,
    endWidgetInteraction: overlayState.endWidgetInteraction,
  })

  const handlers = { ...dragHandlers, ...resizeHandlers, ...scaleHandlers, ...rotateHandlers }
  const selectedRenderedGeometryVersion = useMemo(() => {
    if (!selection.effectiveSelectedWidgetIds.length) {
      return 'none'
    }

    return selection.effectiveSelectedWidgetIds
      .map((widgetId) => {
        const widget = overlayState.renderedWidgetMap[widgetId]
        if (!widget) {
          return 'missing'
        }

        return buildRenderedGeometrySignature(widget, renderGeometryModels[widgetId])
      })
      .join('|')
  }, [overlayState.renderedWidgetMap, renderGeometryModels, selection.effectiveSelectedWidgetIds])

  // Capability flags
  const selectedWidget = selection.selectedWidget
  const hasSingleSelection = Boolean(selectedWidget) && !selection.isGroupSelection
  const isBackdropSelected = isBackdropWidget(selectedWidget)
  const isFramedSelected = isFramedWidget(selectedWidget)
  const selectedDisplayType = selectedWidget?.data?.display_type

  const canResizeSelected = hasSingleSelection && isFramedSelected
  const showEdgeResizeHandles = canResizeSelected && selectedWidget?.type === 'elevation' && !isUniformResizeDisplayType(selectedDisplayType)
  const canScaleSelected = hasSingleSelection && !isFramedSelected
  const canRotateSelected = hasSingleSelection && selectedWidget?.type === 'course'
  const maintainAspectRatio =
    hasSingleSelection &&
    (isUniformResizeDisplayType(selectedDisplayType) ||
      (isBackdropSelected && selectedDisplayType === 'circle') ||
      selectedWidget?.type === 'course' ||
      !isFramedSelected)

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
      sceneFont: globalDefaults?.font_text,
      sceneFontSize: globalDefaults?.font_size,
      sceneStyle: overlayState.sceneStyle,
      valueFont,
      sceneSize: overlayState.sceneSize,
    }),
    [globalDefaults?.font_text, globalDefaults?.font_size, overlayState.sceneStyle, valueFont, overlayState.sceneSize],
  )
  const canvasDisplayProps = useMemo(
    () => ({ displayScale, globalScale: overlayState.globalScale, globalOpacity: overlayState.globalOpacity, backgroundMode, gridVisible }),
    [displayScale, overlayState.globalScale, overlayState.globalOpacity, backgroundMode, gridVisible],
  )
  const canvasDataProps = useMemo(
    () => ({
      widgets: overlayState.canvasWidgets,
      activity,
      previewSecond: overlayState.previewSecond,
      metricPreviewModels,
      textPreviewModels,
      renderGeometryModels,
      exportRange: overlayState.previewExportRange,
    }),
    [
      activity,
      metricPreviewModels,
      overlayState.previewExportRange,
      overlayState.previewSecond,
      overlayState.canvasWidgets,
      renderGeometryModels,
      textPreviewModels,
    ],
  )
  const canvasCallbacks = useMemo(
    () => ({
      setSceneElement: overlayState.setSceneElement,
      handleWidgetMouseDown,
      setHoveredWidgetId,
      widgetRefCallbacks: overlayState.widgetRefCallbacks,
    }),
    [overlayState.setSceneElement, handleWidgetMouseDown, overlayState.widgetRefCallbacks],
  )

  if (!config) return <EmptyOverlayState />

  return (
    <div ref={viewportRef} className="relative flex h-full flex-1 overflow-hidden">
      <CanvasStatusBadges
        height={overlayState.sceneSize.height}
        showProjectStatus={showProjectStatus}
        status={projectStatus}
        width={overlayState.sceneSize.width}
      />
      <CanvasToolbar
        editorShell={editorShell}
        importedBackgroundImageFilename={importedBackgroundImageFilename}
        importedVideoFilename={importedVideoFilename}
        undoRedoControls={undoRedoControls}
      />
      <div ref={scrollViewportRef} className="absolute left-0 right-0 top-12 bottom-0 overflow-auto" onWheel={handleWheel}>
        <div
          ref={setStageElement}
          data-testid="overlay-editor-stage"
          className="relative grid min-h-full min-w-full w-max place-items-center overflow-visible p-4"
          onMouseDown={handleSceneMouseDown}
        >
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
              <OverlayCanvas
                sceneProps={canvasSceneProps}
                displayProps={canvasDisplayProps}
                dataProps={canvasDataProps}
                callbacks={canvasCallbacks}
              />
              <OverlayMoveable
                moveableRef={overlayState.moveableRef}
                selectedTarget={selection.selectedTarget}
                selectedTargets={selection.selectedTargets}
                geometryVersion={selectedRenderedGeometryVersion}
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
                interactionType={overlayState.activeWidgetInteraction?.type ?? null}
              />
            </div>
            <WidgetBadgeLayer
              displayScale={displayScale}
              hoveredWidgetId={hoveredWidgetId}
              renderGeometryModels={renderGeometryModels}
              selectedWidgetIds={selection.selectedWidgetIds}
              widgets={overlayState.canvasWidgets}
            />
          </div>
          {selectionRect ? (
            <div
              data-testid="selection-rect"
              className="pointer-events-none absolute z-40 border border-primary/70 bg-primary/10"
              style={{
                left: selectionRect.x,
                top: selectionRect.y,
                width: selectionRect.width,
                height: selectionRect.height,
              }}
            />
          ) : null}
        </div>
      </div>
    </div>
  )
}

function OverlayEditor(props) {
  return <OverlayEditorContent {...props} />
}

export default memo(OverlayEditor)
