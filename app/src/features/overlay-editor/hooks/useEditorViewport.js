/**
 * Viewport measurement, cursor-anchored zoom, scrolling, and scene snap guides.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { clamp } from '@/lib/utils'
import { getEditorGridSize, getGridGuidelines } from '../utils/overlayEditorUtils'
import { MOVEABLE_ZOOM, VIEWPORT_PADDING, ZOOM_MAX, ZOOM_MIN } from '../data/overlayEditorConstants'

const WHEEL_LINE_PIXELS = 16

function getZoomAnchor(sceneElement, clientX, clientY) {
  if (!sceneElement) return null
  const bounds = sceneElement.getBoundingClientRect()
  if (bounds.width <= 0 || bounds.height <= 0) return null
  return { clientX, clientY, sceneXRatio: (clientX - bounds.left) / bounds.width, sceneYRatio: (clientY - bounds.top) / bounds.height }
}

function getHorizontalWheelDelta(event) {
  const delta = event.deltaX !== 0 ? event.deltaX : event.deltaY
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) return delta * WHEEL_LINE_PIXELS
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) return delta * event.currentTarget.clientWidth
  return delta
}

function getNextZoomLevel(zoomLevel, deltaY) {
  const delta = deltaY < 0 ? 0.05 : -0.05
  return clamp(Number((zoomLevel + delta).toFixed(2)), ZOOM_MIN, ZOOM_MAX)
}

function setViewportMeasurement(viewportNode, setViewportSize, [entry]) {
  setViewportSize({ width: entry.contentRect.width || viewportNode.clientWidth, height: entry.contentRect.height || viewportNode.clientHeight })
}

function observeViewport(viewportRef, setViewportSize) {
  const node = viewportRef.current
  if (!node || typeof ResizeObserver === 'undefined') return
  const observer = new ResizeObserver(setViewportMeasurement.bind(null, node, setViewportSize))
  observer.observe(node)
  return observer.disconnect.bind(observer)
}

function getFitScale(viewportSize, contentSize) {
  const width = Math.max(viewportSize.width - VIEWPORT_PADDING, 1)
  const height = Math.max(viewportSize.height - VIEWPORT_PADDING, 1)
  return Math.min(width / contentSize.width, height / contentSize.height, 1)
}

function handleWheel({ onZoomLevelChange, sceneElement, zoomAnchorRef, zoomLevel }, event) {
  const match = matchKeyboardShortcut(event, 'editor')
  if (match?.commandId === 'editor.wheelZoom') {
    if (event.deltaY === 0) return
    event.preventDefault()
    const nextZoomLevel = getNextZoomLevel(zoomLevel, event.deltaY)
    zoomAnchorRef.current = nextZoomLevel === zoomLevel ? null : getZoomAnchor(sceneElement, event.clientX, event.clientY)
    onZoomLevelChange(nextZoomLevel)
  } else if (match?.commandId === 'editor.wheelScroll') {
    event.preventDefault()
    event.currentTarget.scrollLeft += getHorizontalWheelDelta(event)
  }
}

function applyZoomAnchor(zoomAnchorRef, scrollViewportRef, sceneElement) {
  const anchor = zoomAnchorRef.current
  if (!anchor) return
  zoomAnchorRef.current = null
  const bounds = sceneElement.getBoundingClientRect()
  const viewport = scrollViewportRef.current
  viewport.scrollLeft += bounds.left + bounds.width * anchor.sceneXRatio - anchor.clientX
  viewport.scrollTop += bounds.top + bounds.height * anchor.sceneYRatio - anchor.clientY
}

/** @param {object} options Content/scene dimensions, zoom state, snap mode, and rendered scene. @returns {object} Viewport refs, zoom, and guidelines. */
export function useEditorViewport({ contentSize, onZoomLevelChange, sceneElement, sceneSize, snapToGrid, zoomLevel }) {
  const viewportRef = useRef(null)
  const scrollViewportRef = useRef(null)
  const zoomAnchorRef = useRef(null)
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })
  useEffect(() => observeViewport(viewportRef, setViewportSize), [])
  const fitScale = useMemo(() => getFitScale(viewportSize, contentSize), [contentSize, viewportSize])
  const displayScale = fitScale * zoomLevel
  const wheelHandler = useCallback(
    (event) => handleWheel({ onZoomLevelChange, sceneElement, zoomAnchorRef, zoomLevel }, event),
    [onZoomLevelChange, sceneElement, zoomLevel],
  )
  useLayoutEffect(() => applyZoomAnchor(zoomAnchorRef, scrollViewportRef, sceneElement), [displayScale, sceneElement])
  const gridSize = getEditorGridSize(sceneSize)
  const horizontalGuidelines = useMemo(() => getGridGuidelines(sceneSize.height, gridSize, snapToGrid), [gridSize, sceneSize.height, snapToGrid])
  const verticalGuidelines = useMemo(() => getGridGuidelines(sceneSize.width, gridSize, snapToGrid), [gridSize, sceneSize.width, snapToGrid])
  return {
    displayScale,
    handleWheel: wheelHandler,
    scrollViewportRef,
    viewportRef,
    horizontalGuidelines,
    verticalGuidelines,
    moveableZoom: MOVEABLE_ZOOM / displayScale / 2,
  }
}
