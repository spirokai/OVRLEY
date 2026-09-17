/**
 * Owns pointer capture and transient video-local landmark drag state.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { clamp } from '@/lib/utils'
import { pointerToSecond } from '@/features/player/utils/timelineGeometry'

function isPrimaryButton(event) {
  return event.button === undefined || event.button === 0
}

function getContainerRect(metrics, event) {
  return metrics.containerElement?.getBoundingClientRect?.() ?? event.currentTarget.getBoundingClientRect()
}

function getLandmarkVideoSecond({ event, metrics, videoSyncOffsetSeconds, videoDuration }) {
  const rect = getContainerRect(metrics, event)
  const timelineSecond = pointerToSecond({
    clientX: event.clientX,
    rect,
    timelineMinimum: metrics.timelineMinimum,
    totalDuration: metrics.totalDuration,
    viewEnd: metrics.viewEnd,
    viewStart: metrics.viewStart,
    widthPx: metrics.widthPx,
  })
  return clamp(timelineSecond - videoSyncOffsetSeconds, 0, videoDuration)
}

/**
 * Manages video-local landmark dragging against the existing timeline metrics.
 *
 * @param {object} options Drag inputs.
 * @param {boolean} [options.enabled=false] Whether landmark interaction is active.
 * @param {object|null} options.containerElement Measured timeline element.
 * @param {number} options.viewStart Current timeline viewport start.
 * @param {number} options.viewEnd Current timeline viewport end.
 * @param {number} options.widthPx Measured timeline width.
 * @param {number} options.timelineMinimum Minimum legal timeline second.
 * @param {number} options.totalDuration Maximum legal timeline second.
 * @param {number|null} options.videoDuration Imported video duration.
 * @param {number} options.videoSyncOffsetSeconds Committed video offset.
 * @param {function} options.scrubTo Timeline scrub callback for handle clicks.
 * @param {function} options.moveLandmark Store action for committing a landmark position.
 * @param {function} [options.followSecond] Existing viewport edge-follow callback.
 * @returns {{dragPreview: object|null, getLandmarkPointerProps: function}} Landmark drag model.
 */
export default function useVideoSyncLandmarkDrag({
  enabled = false,
  containerElement,
  viewStart,
  viewEnd,
  widthPx,
  timelineMinimum,
  totalDuration,
  videoDuration,
  videoSyncOffsetSeconds,
  scrubTo,
  moveLandmark,
  followSecond,
}) {
  const [dragPreview, setDragPreview] = useState(null)
  const dragRef = useRef(null)
  const completedDragMovedRef = useRef(false)
  const metricsRef = useRef({ containerElement, timelineMinimum, totalDuration, viewEnd, viewStart, widthPx })

  useEffect(() => {
    metricsRef.current = { containerElement, timelineMinimum, totalDuration, viewEnd, viewStart, widthPx }
  }, [containerElement, timelineMinimum, totalDuration, viewEnd, viewStart, widthPx])

  const followPointerAtEdge = useCallback(
    (clientX) => {
      const metrics = metricsRef.current
      const rect = metrics.containerElement?.getBoundingClientRect?.()
      const right = rect?.right ?? (rect?.left ?? 0) + (rect?.width ?? 0)
      if (!rect || rect.width <= 0 || (clientX >= (rect.left ?? 0) && clientX <= right)) return

      const pointerSecond = pointerToSecond({
        clientX,
        rect,
        timelineMinimum: metrics.timelineMinimum,
        totalDuration: metrics.totalDuration,
        viewEnd: metrics.viewEnd,
        viewStart: metrics.viewStart,
        widthPx: metrics.widthPx,
      })
      const nextViewport = followSecond?.(pointerSecond, metrics.timelineMinimum)
      if (nextViewport) {
        metrics.viewStart = nextViewport.viewport.viewStart
        metrics.viewEnd = nextViewport.viewport.viewEnd
      }
    },
    [followSecond],
  )

  const updateLandmarkPreview = useCallback(
    (event) => {
      const drag = dragRef.current
      if (!enabled || !drag || drag.pointerId !== event.pointerId || videoDuration === null) return
      event.stopPropagation()
      drag.moved = true
      followPointerAtEdge(event.clientX)
      const videoSecond = getLandmarkVideoSecond({
        event,
        metrics: metricsRef.current,
        videoDuration,
        videoSyncOffsetSeconds,
      })
      drag.videoSecond = videoSecond
      setDragPreview({ id: drag.id, videoSecond })
    },
    [enabled, followPointerAtEdge, videoDuration, videoSyncOffsetSeconds],
  )

  const endLandmarkDrag = useCallback(
    (event, commit) => {
      const drag = dragRef.current
      if (!drag || drag.pointerId !== event.pointerId) return
      event.stopPropagation()
      if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
      completedDragMovedRef.current = drag.moved
      dragRef.current = null
      setDragPreview(null)
      if (commit && drag.moved) moveLandmark(drag.id, drag.videoSecond)
    },
    [moveLandmark],
  )

  const getLandmarkPointerProps = useCallback(
    (landmark) => {
      const onPointerDown = (event) => {
        event.stopPropagation()
        event.preventDefault()
        if (!enabled || !isPrimaryButton(event) || videoDuration === null) return
        event.currentTarget.setPointerCapture?.(event.pointerId)
        dragRef.current = {
          id: landmark.id,
          moved: false,
          pointerId: event.pointerId,
          videoSecond: landmark.videoSecond,
        }
        setDragPreview({ id: landmark.id, videoSecond: landmark.videoSecond })
      }

      return {
        onClick: (event) => {
          event.stopPropagation()
          if (!completedDragMovedRef.current) scrubTo(landmark.timelineSecond)
          completedDragMovedRef.current = false
        },
        onPointerCancel: (event) => endLandmarkDrag(event, false),
        onPointerDown,
        onPointerMove: updateLandmarkPreview,
        onPointerUp: (event) => endLandmarkDrag(event, true),
      }
    },
    [enabled, endLandmarkDrag, scrubTo, updateLandmarkPreview, videoDuration],
  )

  useEffect(() => {
    if (!enabled) {
      dragRef.current = null
      setDragPreview(null)
    }
  }, [enabled])

  return { dragPreview, getLandmarkPointerProps }
}
