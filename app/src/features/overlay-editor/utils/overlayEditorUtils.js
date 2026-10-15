/**
 * Pure editor grid, viewport, and selection-handle presentation.
 */

import { getEffectiveWidgetData } from '@/lib/template/template-state'
import { isFramedWidget } from '@/lib/widget/display-type-behavior'
import { VIDEO_SYNC_PREVIEW_SCREEN_GAP } from '@/features/video-sync'
import { EDITOR_GRID_DIVISIONS } from '../data/overlayEditorConstants'

/** @param {{width: number, height: number}} sceneSize Scene dimensions. @returns {number} Grid spacing in scene pixels. */
export function getEditorGridSize(sceneSize) {
  return Math.max(1, Math.round(Math.min(sceneSize.width, sceneSize.height) / EDITOR_GRID_DIVISIONS))
}

function compareNumbers(left, right) {
  return left - right
}

/** @param {number} max Scene dimension. @param {number} gridSize Grid spacing. @param {boolean} enabled Whether grid snapping is enabled. @returns {number[]} Sorted snap guidelines. */
export function getGridGuidelines(max, gridSize, enabled) {
  const guidelines = [0, max / 2, max]
  if (enabled) {
    for (let value = gridSize; value < max; value += gridSize) guidelines.push(value)
  }
  return [...new Set(guidelines)].sort(compareNumbers)
}

/** @param {object} sceneSize Scene dimensions. @param {boolean} videoSyncMode Whether both sync screens are visible. @returns {object} Viewport content size. */
export function getViewportContentSize(sceneSize, videoSyncMode) {
  return videoSyncMode ? { width: sceneSize.width * 2 + VIDEO_SYNC_PREVIEW_SCREEN_GAP, height: sceneSize.height } : sceneSize
}

/** @param {object|null} widget Primary selection. @param {boolean} isGroupSelection Whether multiple widgets are selected. @returns {object} Resize/rotation handle options. */
export function getWidgetResizeOptions(widget, isGroupSelection) {
  if (widget === null) {
    return { canResizeSelected: false, canRotateSelected: false, maintainAspectRatio: false, showEdgeResizeHandles: false }
  }
  if (isGroupSelection) {
    return { canResizeSelected: true, canRotateSelected: false, maintainAspectRatio: true, showEdgeResizeHandles: false }
  }
  return {
    canResizeSelected: true,
    canRotateSelected: widget.type === 'course' || widget.type === 'raster',
    showEdgeResizeHandles: widget.type === 'raster' || widget.type === 'elevation',
    maintainAspectRatio:
      ['arc', 'corner', 'lean_angle', 'g_force'].includes(widget.data.display_type) ||
      (widget.type === 'backdrop' && widget.data.display_type === 'circle') ||
      widget.type === 'course' ||
      !isFramedWidget(widget),
  }
}

function materializeWidget(globalDefaults, widget) {
  return { ...widget, data: getEffectiveWidgetData(widget, globalDefaults) }
}

/** @param {object[]} rawWidgets Config widgets. @param {object} globalDefaults Validated defaults. @returns {object[]} Effective preview widgets. */
export function materializeWidgets(rawWidgets, globalDefaults) {
  return rawWidgets.map(materializeWidget.bind(null, globalDefaults))
}

/** @param {object[]} widgets Ordered config widgets. @returns {string[]} Canonical widget IDs. */
export function getOrderedWidgetIds(widgets) {
  const ids = []
  for (const widget of widgets) ids.push(widget.id)
  return ids
}

/** @param {object[]} widgets Preview widgets. @returns {object} Widgets keyed by ID. */
export function getWidgetMap(widgets) {
  const map = {}
  for (const widget of widgets) map[widget.id] = widget
  return map
}

/** @param {object} config Validated config. @returns {object} Scene dimensions. */
export function getSceneSize(config) {
  return { width: config.scene.width, height: config.scene.height }
}

/** @param {object} exportRange Committed export range. @param {string|null} importedVideoPath Imported video. @param {number} importedVideoDuration Duration. @param {number} videoSyncOffsetSeconds Offset. @returns {object} Preview range. */
export function getPreviewExportRange(exportRange, importedVideoPath, importedVideoDuration, videoSyncOffsetSeconds) {
  if (importedVideoPath === null || exportRange.type === 'custom') return exportRange
  return { type: 'custom', from: videoSyncOffsetSeconds, to: videoSyncOffsetSeconds + importedVideoDuration }
}
