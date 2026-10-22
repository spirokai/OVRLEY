/**
 * Resolved widgets, shared drafts/nodes, scene dimensions, and preview timing.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { resolveVideoSyncMarkControls } from '@/features/video-sync'
import { useWasmWidgetInteractions } from '@/features/wasm-widgets/hooks/useWasmWidgetInteractions'
import { buildWasmScene } from '@/features/wasm-widgets/utils/previewScene'
import {
  getEditorGridSize,
  getOrderedWidgetIds,
  getPreviewExportRange,
  getSceneSize,
  getViewportContentSize,
  getWidgetMap,
  getWidgetResizeOptions,
  materializeWidgets,
} from '../utils/overlayEditorUtils'
import useWidgetSelection from './useWidgetSelection'
import { useEditorViewport } from './useEditorViewport'
import { useEditorKeyboard } from './useEditorKeyboard'
import { useOverlayPreviewStore } from '@/hooks/useAppStoreSelectors'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { updateWidgetInConfig, updateWidgetsInConfig } from '@/lib/widget/widget-config'
import { applyWidgetDrafts } from '@/lib/widget/widget-resolver'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/preview-perf'
import { useWidgetDraftView } from './useWidgetDraftState'

function getWidgetRefCallbacks(widgetLiveEdits, orderedWidgetIds) {
  const callbacks = {}
  for (const id of orderedWidgetIds) callbacks[id] = widgetLiveEdits.getWidgetNodeRef(id)
  return callbacks
}

function commitWidgetUpdate(config, onConfigChange, widgetId, updates) {
  onConfigChange(updateWidgetInConfig(config, widgetId, updates))
}

function commitWidgetUpdates(config, onConfigChange, updatesById) {
  onConfigChange(updateWidgetsInConfig(config, updatesById))
}

function recordPreviewUpdate() {
  incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
}

/** @param {object} options Committed config, global defaults, commit callback, and shared edits. @returns {object} Editor preview state. */
export default function useOverlayEditorState({
  config,
  globalDefaults,
  onConfigChange,
  widgetLiveEdits,
  editorShell,
  zoomLevel,
  onZoomLevelChange,
  gridVisible,
  snapToGrid,
  videoSyncMode = false,
  videoSyncMarkControls = null,
}) {
  const {
    selectedSecond,
    exportRange,
    importedVideoPath,
    importedVideoDuration,
    videoSyncOffsetSeconds,
    parsedActivity,
    disabledLaps,
    importedBackgroundImagePath,
    fallbackDurationSeconds,
  } = useOverlayPreviewStore()
  const moveableRef = useRef(null)
  const clipboardRef = useRef(null)
  const gridRef = useRef(null)
  const [sceneElement, setSceneElement] = useState(null)
  const { liveWidgetDrafts, widgetNodes, clearWidgetDrafts, resetWidgetDrafts, setLiveWidgetDraft, setLiveWidgetDrafts } =
    useWidgetDraftView(widgetLiveEdits)

  const rawWidgets = useMemo(() => buildConfigWidgets(config), [config])
  const effectiveWidgets = useMemo(() => materializeWidgets(rawWidgets, globalDefaults), [globalDefaults, rawWidgets])
  const widgets = useMemo(() => applyWidgetDrafts(effectiveWidgets, liveWidgetDrafts), [effectiveWidgets, liveWidgetDrafts])
  const widgetMap = useMemo(() => getWidgetMap(widgets), [widgets])
  const orderedWidgetIds = useMemo(() => getOrderedWidgetIds(rawWidgets), [rawWidgets])
  const widgetRefCallbacks = useMemo(() => getWidgetRefCallbacks(widgetLiveEdits, orderedWidgetIds), [orderedWidgetIds, widgetLiveEdits])
  const sceneSize = useMemo(() => getSceneSize(config), [config])
  const previewExportRange = useMemo(
    () => getPreviewExportRange(exportRange, importedVideoPath, importedVideoDuration, videoSyncOffsetSeconds),
    [exportRange, importedVideoDuration, importedVideoPath, videoSyncOffsetSeconds],
  )
  const commitUpdate = useCallback((widgetId, updates) => commitWidgetUpdate(config, onConfigChange, widgetId, updates), [config, onConfigChange])
  const commitUpdates = useCallback((updatesById) => commitWidgetUpdates(config, onConfigChange, updatesById), [config, onConfigChange])

  useEffect(recordPreviewUpdate, [selectedSecond])
  useEffect(() => resetWidgetDrafts(), [config, resetWidgetDrafts])

  const state = {
    activity: parsedActivity,
    clearWidgetDrafts,
    commitWidgetUpdate: commitUpdate,
    commitWidgetUpdates: commitUpdates,
    config,
    globalDefaults,
    moveableRef,
    orderedWidgetIds,
    previewSecond: selectedSecond,
    widgetMap,
    sceneElement,
    sceneSize,
    setLiveWidgetDraft,
    setLiveWidgetDrafts,
    setSceneElement,
    widgetNodes,
    widgetRefCallbacks,
    widgets,
  }
  const viewportContentSize = useMemo(() => getViewportContentSize(sceneSize, videoSyncMode), [sceneSize, videoSyncMode])
  const viewport = useEditorViewport({ contentSize: viewportContentSize, onZoomLevelChange, sceneElement, sceneSize, snapToGrid, zoomLevel })
  const selection = useWidgetSelection({ ...state, displayScale: viewport.displayScale })
  useEditorKeyboard({
    config,
    editorShell,
    onConfigChange,
    clipboardRef,
    selectedWidgetIds: selection.selectedWidgetIds,
    selectedWidgets: selection.selectedWidgets,
    setWidgetSelection: selection.setWidgetSelection,
  })
  const handlers = useWasmWidgetInteractions({
    selectedWidget: selection.selectedWidget,
    selectedWidgets: selection.selectedWidgets,
    state,
    setGroupGestureType: selection.setGroupGestureType,
    setGroupDragSelectionIds: selection.setGroupDragSelectionIds,
  })
  const previewRasterScale = Math.min(1, viewport.displayScale * window.devicePixelRatio)
  const scene = useMemo(
    () =>
      buildWasmScene({
        globalDefaults,
        sceneSize,
        exportRange: previewExportRange,
        activity: parsedActivity,
        fallbackDurationSeconds,
        previewRasterScale,
      }),
    [globalDefaults, sceneSize, previewExportRange, parsedActivity, fallbackDurationSeconds, previewRasterScale],
  )
  const markControls = resolveVideoSyncMarkControls(videoSyncMarkControls, selectedSecond, videoSyncMode)
  if (videoSyncMode && markControls === null) throw new Error('Video sync editor requires mark controls')
  useEffect(
    () => drawGrid(gridRef, sceneSize, viewport.displayScale, gridVisible && !videoSyncMode),
    [sceneSize, viewport.displayScale, gridVisible, videoSyncMode],
  )
  return {
    activity: parsedActivity,
    disabledLaps,
    globalDefaults,
    previewSecond: selectedSecond,
    sceneSize,
    setSceneElement,
    widgets,
    widgetRefCallbacks,
    viewport,
    viewportContentSize,
    scene,
    gridRef,
    backgroundImageSrc: importedBackgroundImagePath === null ? null : convertFileSrc(importedBackgroundImagePath),
    gridStyle: { transform: `scale(${1 / viewport.displayScale})`, transformOrigin: 'top left' },
    handleSceneMouseDown: selection.handleSceneMouseDown,
    handleWidgetMouseDown: selection.handleWidgetMouseDown,
    handleCanvasMouseOver: selection.handleCanvasMouseOver,
    handleCanvasMouseLeave: selection.handleCanvasMouseLeave,
    widgetBadges: selection.widgetBadges,
    selectionRect: selection.selectionRect,
    setStageElement: selection.setStageElement,
    videoSyncMarkControls: markControls,
    moveableProps: {
      ...getWidgetResizeOptions(selection.selectedWidget, selection.isGroupSelection),
      moveableRef,
      sceneElement,
      selectedTarget: selection.selectedTarget,
      selectedTargets: selection.selectedTargets,
      isGroupSelection: selection.isGroupSelection,
      isGroupDragActive: selection.isGroupDragActive,
      elementGuidelines: selection.elementGuidelines,
      horizontalGuidelines: viewport.horizontalGuidelines,
      verticalGuidelines: viewport.verticalGuidelines,
      zoom: viewport.moveableZoom,
      handlers,
    },
  }
}

function drawGrid(canvasRef, sceneSize, displayScale, visible) {
  const canvas = canvasRef.current
  if (!visible || !canvas) return
  const context = canvas.getContext('2d')
  if (!context) return
  const width = sceneSize.width * displayScale
  const height = sceneSize.height * displayScale
  const gridSize = getEditorGridSize(sceneSize)
  const pixelRatio = window.devicePixelRatio
  canvas.width = Math.max(1, Math.round(width * pixelRatio))
  canvas.height = Math.max(1, Math.round(height * pixelRatio))
  canvas.style.width = `${width}px`
  canvas.style.height = `${height}px`
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0)
  context.clearRect(0, 0, width, height)
  context.strokeStyle = '#003836'
  context.lineWidth = 1
  context.beginPath()
  for (let x = 0; x <= sceneSize.width; x += gridSize) {
    const displayX = Math.min(Math.round(x * displayScale) + 0.5, Math.max(0.5, Math.round(width) - 0.5))
    context.moveTo(displayX, 0)
    context.lineTo(displayX, height)
  }
  for (let y = 0; y <= sceneSize.height; y += gridSize) {
    const displayY = Math.min(Math.round(y * displayScale) + 0.5, Math.max(0.5, Math.round(height) - 0.5))
    context.moveTo(0, displayY)
    context.lineTo(width, displayY)
  }
  context.stroke()
}
