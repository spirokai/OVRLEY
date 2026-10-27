/**
 * Pure editor grid, viewport, and selection-handle presentation.
 */

import { getEffectiveWidgetData } from '@/lib/template/template-state'
import { isFramedWidget } from '@/lib/widget/widget-catalog'
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
    showEdgeResizeHandles: ['raster', 'elevation', 'diving_profile', 'rolling_chart'].includes(widget.type),
    maintainAspectRatio:
      ['arc', 'corner', 'gt_gauge', 'lean_angle', 'heading_compass', 'g_force'].includes(widget.data.display_type) ||
      (widget.type === 'backdrop' && widget.data.display_type === 'circle') ||
      widget.type === 'course' ||
      !isFramedWidget(widget),
  }
}

function materializeWidget(globalDefaults, language, widget) {
  return { ...widget, data: getEffectiveWidgetData(widget, globalDefaults, language) }
}

/** @param {object[]} rawWidgets Config widgets. @param {object} globalDefaults Validated defaults. @returns {object[]} Effective preview widgets. */
export function materializeWidgets(rawWidgets, globalDefaults, language) {
  return rawWidgets.map(materializeWidget.bind(null, globalDefaults, language))
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

/**
 * The export range in the activity time widgets read: the custom range shifted by
 * the activity origin; without one, the telemetry clip's own duration, or the
 * interval covered by the videos.
 * @param {object} options Range inputs.
 * @param {{type: 'all'|'custom', from: number, to: number}} options.exportRange Timeline export range.
 * @param {number} options.activityOriginSecond Timeline second of activity second 0.
 * @param {number|null} options.activityClipDurationSeconds Duration of the clip supplying embedded telemetry, or null with an external activity.
 * @param {{startSecond: number, endSecond: number}|null} options.videoBounds Interval covered by the videos.
 * @returns {{type: 'all'|'custom', from: number, to: number}} Preview export range.
 */
export function resolvePreviewExportRange({ exportRange, activityOriginSecond, activityClipDurationSeconds, videoBounds }) {
  if (exportRange.type === 'custom') {
    return { ...exportRange, from: exportRange.from - activityOriginSecond, to: exportRange.to - activityOriginSecond }
  }
  if (activityClipDurationSeconds !== null) return { type: 'custom', from: 0, to: activityClipDurationSeconds }
  if (videoBounds === null) return exportRange
  return { type: 'custom', from: videoBounds.startSecond, to: videoBounds.endSecond }
}
