/**
 * Owns pointer gesture state for the timeline surface.
 */

import { useCallback, useRef, useState } from 'react'
import { pointerToSecond, viewPxToSeconds } from '../utils/timelineGeometry'

function isPrimaryButton(event) {
  return event.button === undefined || event.button === 0
}

function isDragType(dragRef, dragType) {
  return dragRef.current?.type === dragType
}

/**
 * Owns timeline pointer interactions for scrub, pan, and playhead drags.
 *
 * @param {object} options Gesture command inputs.
 * @param {function} options.scrubTo Preview-scrub command that accepts a timeline second.
 * @param {function} options.commitScrub Final scrub command that accepts a timeline second.
 * @param {function} options.cancelScrub Scrub cancellation command for pending preview work.
 * @returns {object} Drag state, event props, and metric sync command.
 */
export default function useTimelineGestures({ scrubTo, commitScrub, cancelScrub }) {
  // Drag state - one flag suspends viewport auto-follow during any timeline pointer interaction.
  const [isTimelineDragging, setIsTimelineDragging] = useState(false)

  // Active drag ref - pointermove handlers need the drag type without rerendering on every move.
  const dragRef = useRef(null)

  // Metrics ref - pointer math reads the latest viewport/width values without rebuilding handlers.
  const metricsRef = useRef({
    containerElement: null,
    panBy: null,
    timelineMinimum: 0,
    totalDuration: 0,
    viewEnd: 0,
    viewStart: 0,
    widthPx: 0,
  })

  // Metrics sync - the orchestrator updates this after viewport measurement or range changes.
  const updateTimelineMetrics = useCallback((metrics) => {
    metricsRef.current = {
      ...metricsRef.current,
      ...metrics,
    }
  }, [])

  // Coordinate reader - all gestures convert clientX through the same timeline element and viewport.
  const readSecond = useCallback((event) => {
    const metrics = metricsRef.current
    const rect = metrics.containerElement?.getBoundingClientRect?.() ?? event.currentTarget.getBoundingClientRect()

    return pointerToSecond({
      clientX: event.clientX,
      rect,
      viewStart: metrics.viewStart,
      viewEnd: metrics.viewEnd,
      widthPx: metrics.widthPx,
      timelineMinimum: metrics.timelineMinimum,
      totalDuration: metrics.totalDuration,
    })
  }, [])

  // Capture start - pointer capture keeps drags alive even when the pointer leaves the handle.
  const startCapturedDrag = useCallback((event, dragState) => {
    if (!isPrimaryButton(event)) return false
    event.currentTarget.setPointerCapture?.(event.pointerId)
    dragRef.current = dragState
    setIsTimelineDragging(true)
    return true
  }, [])

  // Capture release - all pointer endings clear drag ownership and resume viewport auto-follow.
  const releaseCapturedDrag = useCallback((event) => {
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    dragRef.current = null
    setIsTimelineDragging(false)
  }, [])

  // Axis handlers - clicking or dragging the axis previews scrub and commits on pointer up.
  const axisProps = {
    onPointerDown: useCallback(
      (event) => {
        if (!startCapturedDrag(event, { type: 'scrub' })) return
        scrubTo(readSecond(event))
      },
      [readSecond, scrubTo, startCapturedDrag],
    ),
    onPointerMove: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'scrub')) return
        scrubTo(readSecond(event))
      },
      [readSecond, scrubTo],
    ),
    onPointerUp: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'scrub')) return
        commitScrub(readSecond(event))
        releaseCapturedDrag(event)
      },
      [commitScrub, readSecond, releaseCapturedDrag],
    ),
    onPointerCancel: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'scrub')) return
        cancelScrub()
        releaseCapturedDrag(event)
      },
      [cancelScrub, releaseCapturedDrag],
    ),
  }

  // Pan handlers - lane background drag converts pixel deltas into seconds before moving the viewport.
  // React bubbles events from portalled popovers through the lanes, so presses outside the surface's DOM are ignored.
  const panSurfaceProps = {
    onPointerDown: useCallback(
      (event) => {
        if (!event.currentTarget.contains(event.target)) return
        startCapturedDrag(event, { lastClientX: event.clientX, type: 'pan' })
      },
      [startCapturedDrag],
    ),
    onPointerMove: useCallback((event) => {
      const drag = dragRef.current
      const metrics = metricsRef.current
      if (drag?.type !== 'pan') return

      const deltaSeconds = viewPxToSeconds({
        deltaPx: drag.lastClientX - event.clientX,
        viewStart: metrics.viewStart,
        viewEnd: metrics.viewEnd,
        widthPx: metrics.widthPx,
      })
      if (deltaSeconds === 0) return
      drag.lastClientX = event.clientX
      metrics.panBy?.(deltaSeconds)
    }, []),
    onPointerUp: useCallback(
      (event) => {
        if (dragRef.current?.type !== 'pan') return
        releaseCapturedDrag(event)
      },
      [releaseCapturedDrag],
    ),
    onPointerCancel: useCallback(
      (event) => {
        if (dragRef.current?.type !== 'pan') return
        releaseCapturedDrag(event)
      },
      [releaseCapturedDrag],
    ),
  }

  // Playhead handlers - stop propagation so dragging the handle does not also start a pan/scrub underneath.
  const playheadProps = {
    onPointerDown: useCallback(
      (event) => {
        event.stopPropagation()
        if (!startCapturedDrag(event, { type: 'playhead' })) return
        scrubTo(readSecond(event))
      },
      [readSecond, scrubTo, startCapturedDrag],
    ),
    onPointerMove: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'playhead')) return
        event.stopPropagation()
        scrubTo(readSecond(event))
      },
      [readSecond, scrubTo],
    ),
    onPointerUp: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'playhead')) return
        event.stopPropagation()
        commitScrub(readSecond(event))
        releaseCapturedDrag(event)
      },
      [commitScrub, readSecond, releaseCapturedDrag],
    ),
    onPointerCancel: useCallback(
      (event) => {
        if (!isDragType(dragRef, 'playhead')) return
        event.stopPropagation()
        cancelScrub()
        releaseCapturedDrag(event)
      },
      [cancelScrub, releaseCapturedDrag],
    ),
  }

  // Gesture API - presentational surface spreads these props without knowing interaction rules.
  return {
    axisProps,
    isTimelineDragging,
    panSurfaceProps,
    playheadProps,
    updateTimelineMetrics,
  }
}
