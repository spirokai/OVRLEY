/**
 * Viewport state and resize observer for the overlay editor.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { clamp } from '@/lib/utils'
import { VIEWPORT_PADDING, ZOOM_MAX, ZOOM_MIN } from '../data/overlayEditorConstants'

const WHEEL_LINE_PIXELS = 16

function getZoomAnchor(sceneElement, clientX, clientY) {
  if (!sceneElement) {
    return null
  }

  const sceneBounds = sceneElement.getBoundingClientRect()
  if (sceneBounds.width <= 0 || sceneBounds.height <= 0) {
    return null
  }

  return {
    clientX,
    clientY,
    sceneXRatio: (clientX - sceneBounds.left) / sceneBounds.width,
    sceneYRatio: (clientY - sceneBounds.top) / sceneBounds.height,
  }
}

function getHorizontalWheelDelta(event) {
  const delta = event.deltaX !== 0 ? event.deltaX : event.deltaY
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    return delta * WHEEL_LINE_PIXELS
  }
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    return delta * event.currentTarget.clientWidth
  }
  return delta
}

function getNextZoomLevel(zoomLevel, deltaY) {
  const delta = deltaY < 0 ? 0.05 : -0.05
  return clamp(Number((zoomLevel + delta).toFixed(2)), ZOOM_MIN, ZOOM_MAX)
}

/**
 * Owns viewport measurement, scrolling, panning, and cursor-anchored zoom.
 *
 * @param {object} options
 * @param {Function} options.onZoomLevelChange - Updates the canonical editor zoom level.
 * @param {HTMLElement|null} options.sceneElement - Rendered scene element used for cursor anchoring.
 * @param {{ width: number, height: number }} options.sceneSize - Scene dimensions.
 * @param {number} options.zoomLevel - Canonical editor zoom level.
 * @returns {{ displayScale: number, handleWheel: Function, scrollViewportRef: React.RefObject, viewportRef: React.RefObject }}
 */
export function useEditorViewport({ onZoomLevelChange, sceneElement, sceneSize, zoomLevel }) {
  const viewportRef = useRef(null)
  const scrollViewportRef = useRef(null)
  const zoomAnchorRef = useRef(null)
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 })

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
    const safeWidth = Math.max(viewportSize.width - VIEWPORT_PADDING, 1)
    const safeHeight = Math.max(viewportSize.height - VIEWPORT_PADDING, 1)
    return Math.min(safeWidth / sceneSize.width, safeHeight / sceneSize.height, 1)
  }, [viewportSize, sceneSize])
  const displayScale = fitScale * zoomLevel

  const handleWheel = useCallback(
    (event) => {
      const match = matchKeyboardShortcut(event, 'editor')

      if (match?.commandId === 'editor.wheelZoom') {
        if (event.deltaY === 0) {
          return
        }

        event.preventDefault()
        const nextZoomLevel = getNextZoomLevel(zoomLevel, event.deltaY)
        zoomAnchorRef.current = nextZoomLevel === zoomLevel ? null : getZoomAnchor(sceneElement, event.clientX, event.clientY)
        onZoomLevelChange(nextZoomLevel)
        return
      }

      if (match?.commandId === 'editor.wheelScroll') {
        event.preventDefault()
        event.currentTarget.scrollLeft += getHorizontalWheelDelta(event)
      }
    },
    [onZoomLevelChange, sceneElement, zoomLevel],
  )

  useLayoutEffect(() => {
    const zoomAnchor = zoomAnchorRef.current
    if (!zoomAnchor) {
      return
    }

    zoomAnchorRef.current = null
    const scrollViewport = scrollViewportRef.current
    const sceneBounds = sceneElement.getBoundingClientRect()
    const nextAnchorClientX = sceneBounds.left + sceneBounds.width * zoomAnchor.sceneXRatio
    const nextAnchorClientY = sceneBounds.top + sceneBounds.height * zoomAnchor.sceneYRatio
    scrollViewport.scrollLeft += nextAnchorClientX - zoomAnchor.clientX
    scrollViewport.scrollTop += nextAnchorClientY - zoomAnchor.clientY
  }, [displayScale, sceneElement])

  return { displayScale, handleWheel, scrollViewportRef, viewportRef }
}
