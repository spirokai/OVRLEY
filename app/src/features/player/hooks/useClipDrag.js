/**
 * Owns pointer gesture state for horizontal clip dragging on the timeline.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { getVideosTimelineBounds } from '@/lib/video-program/video-collection'
import { getTimelineMinimum } from '../utils/playerTiming'
import { snapClipDelta, videoIdFromLaneId, viewPxToSeconds } from '../utils/timelineGeometry'

const AUTO_SCROLL_EDGE_RATIO = 0.15
const AUTO_SCROLL_MIN_ZOOM_SCALE = 0.1
const AUTO_SCROLL_MAX_VIEW_SPANS_PER_SECOND = 0.75
const AUTO_SCROLL_MAX_FRAME_SECONDS = 0.05

function isPrimaryButton(event) {
  return event.button === undefined || event.button === 0
}

function getTimelineWidth(metrics) {
  const rectWidth = metrics.containerElement?.getBoundingClientRect?.().width
  return rectWidth > 0 ? rectWidth : metrics.widthPx
}

function getAutoScrollPointerSecond(metrics, clientX, elapsedSeconds) {
  const rect = metrics.containerElement?.getBoundingClientRect?.()
  const widthPx = rect?.width > 0 ? rect.width : metrics.widthPx
  if (widthPx <= 0) return null
  const left = rect?.left ?? 0
  const right = left + widthPx
  const edgeWidth = widthPx * AUTO_SCROLL_EDGE_RATIO
  if (clientX >= left + edgeWidth && clientX <= right - edgeWidth) return null
  const viewSpan = metrics.viewEnd - metrics.viewStart
  const timelineSpan = metrics.totalDuration - metrics.timelineMinimum
  const zoomScale = Math.max(AUTO_SCROLL_MIN_ZOOM_SCALE, Math.min(1, viewSpan / timelineSpan))
  const isLeftEdge = clientX < left + edgeWidth
  const edgeIntensity = Math.min(1, (isLeftEdge ? left + edgeWidth - clientX : clientX - (right - edgeWidth)) / edgeWidth)
  const direction = isLeftEdge ? -1 : 1
  const edgeSecond = isLeftEdge ? metrics.viewStart + viewSpan * AUTO_SCROLL_EDGE_RATIO : metrics.viewEnd - viewSpan * AUTO_SCROLL_EDGE_RATIO
  const deltaSecond = direction * viewSpan * AUTO_SCROLL_MAX_VIEW_SPANS_PER_SECOND * zoomScale * edgeIntensity * elapsedSeconds

  return edgeSecond + deltaSecond
}

function clipEdges(clips) {
  return clips.flatMap((clip) => [clip.offsetSeconds, clip.offsetSeconds + clip.durationSeconds])
}

function roundToTenth(seconds) {
  return Math.round(seconds * 10) / 10
}

/**
 * Resolves which clips move for a lane and which edges they snap to. Dragging a
 * video lane moves that clip; dragging the activity lane moves every clip in the
 * opposite direction because the activity stays at timeline zero.
 */
function resolveLaneMovement(laneId, metrics) {
  const videoId = videoIdFromLaneId(laneId)
  const activityEdges = metrics.hasActivityData ? [0, metrics.activityDurationSeconds] : []
  if (videoId === null) {
    return { videoId, direction: -1, movingClips: metrics.clips, fixedEdges: activityEdges }
  }
  const movingClips = metrics.clips.filter((clip) => clip.id === videoId)
  const otherClips = metrics.clips.filter((clip) => clip.id !== videoId)
  return { videoId, direction: 1, movingClips, fixedEdges: [...activityEdges, ...clipEdges(otherClips)] }
}

/**
 * Owns horizontal drag gestures and keyboard nudges for activity and video clips.
 *
 * @param {object} options Clip commands.
 * @param {function} options.setVideoDragPreview Store action for the transient shift of the dragged clip(s).
 * @param {function} options.setVideoOffset Store action committing one clip's offset.
 * @param {function} options.shiftVideoOffsets Store action committing the same shift for every clip.
 * @returns {object} Per-lane drag props, keyboard nudge commands, dragging flag, and metrics sync command.
 */
export default function useClipDrag({ setVideoDragPreview, setVideoOffset, shiftVideoOffsets }) {
  const [isDragging, setIsDragging] = useState(false)
  const [snapGuidelineSecond, setSnapGuidelineSecond] = useState(null)

  const dragRef = useRef(null)
  const nudgeRef = useRef(null)
  const autoScrollFrameRef = useRef(null)
  const autoScrollTimestampRef = useRef(null)
  const autoScrollTickRef = useRef(null)
  const previewFrameRef = useRef(null)
  const pendingPreviewRef = useRef(null)

  const metricsRef = useRef({
    viewStart: 0,
    viewEnd: 0,
    widthPx: 0,
    timelineMinimum: 0,
    clips: [],
    hasActivityData: false,
    activityDurationSeconds: 0,
  })

  const updateMetrics = useCallback((metrics) => {
    Object.assign(metricsRef.current, metrics)
  }, [])

  const cancelAutoScroll = useCallback(() => {
    autoScrollTimestampRef.current = null
    if (autoScrollFrameRef.current === null) return
    window.cancelAnimationFrame?.(autoScrollFrameRef.current)
    autoScrollFrameRef.current = null
  }, [])

  const cancelPendingPreview = useCallback(() => {
    if (previewFrameRef.current !== null) {
      window.cancelAnimationFrame?.(previewFrameRef.current)
      previewFrameRef.current = null
    }
    pendingPreviewRef.current = null
  }, [])

  const publishPreview = useCallback(
    (preview) => {
      pendingPreviewRef.current = preview
      if (previewFrameRef.current !== null) return
      if (typeof window.requestAnimationFrame !== 'function') {
        pendingPreviewRef.current = null
        setVideoDragPreview(preview)
        return
      }

      previewFrameRef.current = window.requestAnimationFrame(() => {
        previewFrameRef.current = null
        const nextPreview = pendingPreviewRef.current
        pendingPreviewRef.current = null
        if (nextPreview !== null) setVideoDragPreview(nextPreview)
      })
    },
    [setVideoDragPreview],
  )

  // Commit - one clip snaps its start to a tenth of a second; a shared shift keeps relative placement and rounds the earliest clip.
  const commitShift = useCallback(
    ({ videoId, movingClips }, deltaSeconds) => {
      if (videoId !== null) {
        setVideoOffset(videoId, roundToTenth(movingClips[0].offsetSeconds + deltaSeconds))
        return
      }
      const earliestOffset = Math.min(...movingClips.map((clip) => clip.offsetSeconds))
      shiftVideoOffsets(roundToTenth(earliestOffset + deltaSeconds) - earliestOffset)
    },
    [setVideoOffset, shiftVideoOffsets],
  )

  const updateDragOffset = useCallback(
    (clientX) => {
      const drag = dragRef.current
      if (drag?.type !== 'clip-drag') return

      const metrics = metricsRef.current
      const widthPx = getTimelineWidth(metrics)
      const deltaSeconds = viewPxToSeconds({
        deltaPx: clientX - drag.startClientX,
        viewStart: metrics.viewStart,
        viewEnd: metrics.viewEnd,
        widthPx,
      })
      const snap = snapClipDelta({
        proposedDelta: drag.initialDelta + drag.movement.direction * deltaSeconds,
        movingEdges: clipEdges(drag.movement.movingClips),
        fixedEdges: drag.movement.fixedEdges,
        viewEnd: metrics.viewEnd,
        viewStart: metrics.viewStart,
        widthPx,
      })
      const desiredDelta = snap.delta

      drag.desiredDelta = desiredDelta
      metrics.timelineMinimum = getTimelineMinimum({ videoBounds: shiftedBounds(metrics.clips, drag.movement, desiredDelta) })
      const nextDelta =
        drag.movement.videoId === null ? desiredDelta : Math.max(desiredDelta, metrics.viewStart - drag.movement.movingClips[0].offsetSeconds)
      const guidelineSecond = nextDelta === desiredDelta ? snap.guidelineSecond : null
      if (nextDelta === drag.currentDelta && guidelineSecond === drag.guidelineSecond) return

      drag.currentDelta = nextDelta
      drag.guidelineSecond = guidelineSecond
      setSnapGuidelineSecond(guidelineSecond)
      publishPreview({ videoId: drag.movement.videoId, deltaSeconds: nextDelta })
    },
    [publishPreview],
  )

  const autoScrollTick = useCallback(
    (timestamp) => {
      autoScrollFrameRef.current = null
      const drag = dragRef.current
      if (drag?.type !== 'clip-drag') return

      const metrics = metricsRef.current
      metrics.timelineMinimum = getTimelineMinimum({ videoBounds: shiftedBounds(metrics.clips, drag.movement, drag.desiredDelta) })
      const previousTimestamp = autoScrollTimestampRef.current
      const elapsedSeconds = previousTimestamp === null ? 1 / 60 : Math.min((timestamp - previousTimestamp) / 1000, AUTO_SCROLL_MAX_FRAME_SECONDS)
      autoScrollTimestampRef.current = timestamp
      const targetSecond = getAutoScrollPointerSecond(metrics, drag.pointerClientX, elapsedSeconds)
      if (targetSecond === null) {
        autoScrollTimestampRef.current = null
        return
      }

      const followResult = metrics.followSecond?.(targetSecond, metrics.timelineMinimum)
      if (!followResult) return

      drag.initialDelta += drag.movement.direction * followResult.deltaStart
      metrics.viewStart = followResult.viewport.viewStart
      metrics.viewEnd = followResult.viewport.viewEnd
      updateDragOffset(drag.pointerClientX)
      autoScrollFrameRef.current = window.requestAnimationFrame((nextTimestamp) => autoScrollTickRef.current?.(nextTimestamp))
    },
    [updateDragOffset],
  )

  // A queued RAF must call the newest tick callback after dependencies change.
  useEffect(() => {
    autoScrollTickRef.current = autoScrollTick
  }, [autoScrollTick])

  const scheduleAutoScroll = useCallback(() => {
    if (autoScrollFrameRef.current !== null || typeof window.requestAnimationFrame !== 'function') return
    autoScrollFrameRef.current = window.requestAnimationFrame(autoScrollTick)
  }, [autoScrollTick])

  const endDrag = useCallback(
    (event, commit) => {
      const drag = dragRef.current
      if (drag?.type !== 'clip-drag' || drag.pointerId !== event.pointerId) return
      event.stopPropagation()
      cancelAutoScroll()
      cancelPendingPreview()
      event.currentTarget.releasePointerCapture?.(event.pointerId)
      dragRef.current = null
      setIsDragging(false)
      setSnapGuidelineSecond(null)

      const metrics = metricsRef.current
      const snap = snapClipDelta({
        proposedDelta: drag.currentDelta,
        movingEdges: clipEdges(drag.movement.movingClips),
        fixedEdges: drag.movement.fixedEdges,
        viewEnd: metrics.viewEnd,
        viewStart: metrics.viewStart,
        widthPx: getTimelineWidth(metrics),
      })
      if (commit) commitShift(drag.movement, snap.delta)
      setVideoDragPreview(null)
    },
    [cancelAutoScroll, cancelPendingPreview, commitShift, setVideoDragPreview],
  )

  useEffect(
    () => () => {
      cancelAutoScroll()
      cancelPendingPreview()
      setVideoDragPreview(null)
    },
    [cancelAutoScroll, cancelPendingPreview, setVideoDragPreview],
  )

  // Keyboard nudges - accumulate a transient shift for one lane and commit it as one edit.
  const nudgeLane = useCallback(
    (laneId, deltaSeconds) => {
      const metrics = metricsRef.current
      if (metrics.clips.length === 0) return
      const nudge =
        nudgeRef.current?.laneId === laneId ? nudgeRef.current : { laneId, movement: resolveLaneMovement(laneId, metrics), deltaSeconds: 0 }
      nudge.deltaSeconds += nudge.movement.direction * deltaSeconds
      nudgeRef.current = nudge
      setVideoDragPreview({ videoId: nudge.movement.videoId, deltaSeconds: nudge.deltaSeconds })
    },
    [setVideoDragPreview],
  )

  const commitNudge = useCallback(() => {
    const nudge = nudgeRef.current
    if (nudge === null) return
    nudgeRef.current = null
    commitShift(nudge.movement, nudge.deltaSeconds)
    setVideoDragPreview(null)
  }, [commitShift, setVideoDragPreview])

  const getLaneDragProps = useCallback(
    (laneId) => ({
      onPointerDown: (event) => {
        if (!isPrimaryButton(event)) return
        const metrics = metricsRef.current
        if (metrics.clips.length === 0) return
        event.stopPropagation()
        event.preventDefault()
        event.currentTarget.setPointerCapture?.(event.pointerId)
        cancelAutoScroll()
        cancelPendingPreview()
        const movement = resolveLaneMovement(laneId, metrics)
        setVideoDragPreview({ videoId: movement.videoId, deltaSeconds: 0 })
        setSnapGuidelineSecond(null)
        dragRef.current = {
          type: 'clip-drag',
          movement,
          initialDelta: 0,
          currentDelta: 0,
          desiredDelta: 0,
          pointerId: event.pointerId,
          pointerClientX: event.clientX,
          startClientX: event.clientX,
          guidelineSecond: null,
        }
        setIsDragging(true)
      },
      onPointerMove: (event) => {
        const drag = dragRef.current
        if (drag?.type !== 'clip-drag' || drag.pointerId !== event.pointerId) return
        event.stopPropagation()
        drag.pointerClientX = event.clientX
        updateDragOffset(event.clientX)
        scheduleAutoScroll()
      },
      onPointerUp: (event) => endDrag(event, true),
      onPointerCancel: (event) => endDrag(event, false),
    }),
    [cancelPendingPreview, cancelAutoScroll, endDrag, scheduleAutoScroll, setVideoDragPreview, updateDragOffset],
  )

  return {
    commitNudge,
    getLaneDragProps,
    isDragging,
    nudgeLane,
    snapGuidelineSecond,
    updateMetrics,
  }
}

function shiftedBounds(clips, movement, deltaSeconds) {
  return getVideosTimelineBounds(
    clips.map((clip) =>
      movement.videoId === null || clip.id === movement.videoId ? { ...clip, offsetSeconds: clip.offsetSeconds + deltaSeconds } : clip,
    ),
  )
}
