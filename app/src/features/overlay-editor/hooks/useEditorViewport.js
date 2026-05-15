/**
 * Viewport state and resize observer for the overlay editor.
 */

import { useEffect, useMemo, useRef, useState } from 'react'
import { VIEWPORT_PADDING } from '../data/overlayEditorConstants'

/**
 * Provides viewport ref, observed dimensions, and computed fit scale.
 *
 * @param {object} sceneSize - Scene dimensions ({ width, height }).
 * @returns {{ viewportRef: React.RefObject, viewportSize: {width: number, height: number}, fitScale: number }}
 */
export function useEditorViewport(sceneSize) {
  const viewportRef = useRef(null)
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

  return { viewportRef, viewportSize, fitScale }
}
