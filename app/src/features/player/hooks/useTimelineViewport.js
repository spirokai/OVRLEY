/**
 * Owns timeline viewport state, DOM measurement, fit targets, ticks, zoom, pan, and auto-follow.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { roundToDevicePixel } from '../utils/timelineGeometry'
import { getTimelineMinimum } from '../utils/playerTiming'
import {
  buildFitTargets,
  clampToView,
  computeTimelineTicks,
  fitToFull,
  followPlayhead,
  getMatchingFitTargetId,
  panViewport,
  zoomRange,
} from '../utils/timelineViewport'

function getDevicePixelRatio() {
  if (typeof window === 'undefined') return 1
  return window.devicePixelRatio || 1
}

function getLeftPercent(x, widthPx) {
  return widthPx > 0 ? `${(x / widthPx) * 100}%` : '0%'
}

/**
 * Owns timeline viewport state, measurement, fit targets, ticks, zoom, pan, and playback follow.
 *
 * @param {object} options Timeline viewport inputs.
 * @param {number} options.totalDuration Total playable duration.
 * @param {boolean} [options.hasVideo=false] Whether a video lane is present.
 * @param {number} [options.videoSyncOffsetSeconds=0] Committed timeline second where the video starts.
 * @param {number} [options.videoSyncOffsetPreviewSeconds] Transient timeline second used during drag preview.
 * @param {number} [options.importedVideoDuration=0] Imported video duration in seconds.
 * @param {boolean} [options.hasActivityData=false] Whether activity metadata is loaded.
 * @param {number} [options.activityDurationSeconds=0] Activity duration in seconds.
 * @param {number} [options.fallbackDurationSeconds=0] Fallback duration for template-only timelines.
 * @param {boolean} [options.isPlaying=false] Whether playback is active.
 * @param {number} [options.playheadSecond=0] Current playhead second used as zoom/follow pivot.
 * @param {boolean} [options.isDragging=false] Whether a pointer interaction should suspend auto-follow.
 * @returns {object} Viewport state, geometry, and commands.
 */
export default function useTimelineViewport({
  totalDuration,
  hasVideo = false,
  videoSyncOffsetSeconds = 0,
  videoSyncOffsetPreviewSeconds = videoSyncOffsetSeconds,
  importedVideoDuration = 0,
  hasActivityData = false,
  activityDurationSeconds = 0,
  fallbackDurationSeconds = 0,
  isPlaying = false,
  playheadSecond = 0,
  isDragging = false,
}) {
  const timelineMinimum = getTimelineMinimum({ hasVideo, videoSyncOffsetSeconds: videoSyncOffsetPreviewSeconds })
  // Duration ref - viewport actions use the latest duration without recreating every callback.
  const totalDurationRef = useRef(totalDuration)
  const timelineMinimumRef = useRef(timelineMinimum)
  const [containerElement, setContainerElement] = useState(null)
  const [widthPx, setWidthPx] = useState(0)
  const [viewport, setViewport] = useState(() => fitToFull(totalDuration, timelineMinimum))
  const viewportRef = useRef(viewport)

  // Stable follow callbacks read this ref synchronously while returning viewport deltas.
  useEffect(() => {
    viewportRef.current = viewport
  }, [viewport])

  // Callback ref - measurement starts when the timeline actually mounts, including after hidden initial renders.
  const containerRef = useCallback((element) => {
    setContainerElement(element)
  }, [])

  // Media identity - structural media changes reset stale zoom/pan state, while sync timing changes preserve it.
  const mediaIdentity = [hasVideo, importedVideoDuration, hasActivityData, activityDurationSeconds, fallbackDurationSeconds].join('|')

  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  useEffect(() => {
    timelineMinimumRef.current = timelineMinimum
  }, [timelineMinimum])

  // Full-range reset - loading different media should never leave the user stranded in an old viewport.
  useEffect(() => {
    setViewport(fitToFull(totalDurationRef.current, timelineMinimumRef.current))
  }, [mediaIdentity])

  // Duration changes can shorten the current viewport without changing its zoom level.
  useEffect(() => {
    if (isDragging) return
    setViewport((previousViewport) =>
      clampToView(previousViewport.viewStart, previousViewport.viewEnd, totalDurationRef.current, timelineMinimumRef.current),
    )
  }, [isDragging, timelineMinimum, totalDuration])

  // Width measurement - immediate rect reads avoid invisible geometry before ResizeObserver fires.
  useEffect(() => {
    if (!containerElement) {
      setWidthPx(0)
      return undefined
    }

    const measureWidth = () => {
      const rect = containerElement.getBoundingClientRect?.()
      setWidthPx(rect?.width || 0)
    }

    measureWidth()
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener?.('resize', measureWidth)
      return () => window.removeEventListener?.('resize', measureWidth)
    }
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setWidthPx(entry.contentRect.width || containerElement.getBoundingClientRect?.().width || 0)
      }
    })

    observer.observe(containerElement)
    return () => observer.disconnect()
  }, [containerElement])

  const didMountRef = useRef(false)

  // Playback follow - while playing and not dragging, keep the playhead visible in a zoomed viewport.
  useEffect(() => {
    if (!didMountRef.current) {
      didMountRef.current = true
      return
    }
    if (!isPlaying || isDragging) return

    setViewport((previousViewport) =>
      followPlayhead({
        playheadSecond,
        timelineMinimum: timelineMinimumRef.current,
        viewStart: previousViewport.viewStart,
        viewEnd: previousViewport.viewEnd,
        totalDuration: totalDurationRef.current,
      }),
    )
  }, [isDragging, isPlaying, playheadSecond])

  // Fit targets - tabs are derived from canonical ranges so there is no separate active-tab state.
  const fitTargets = useMemo(
    () =>
      buildFitTargets({
        totalDuration,
        widthPx,
        hasVideo,
        videoSyncOffsetSeconds,
        importedVideoDuration,
        hasActivityData,
        activityDurationSeconds,
        fallbackDurationSeconds,
      }),
    [
      activityDurationSeconds,
      fallbackDurationSeconds,
      hasActivityData,
      hasVideo,
      importedVideoDuration,
      totalDuration,
      videoSyncOffsetSeconds,
      widthPx,
    ],
  )

  const displayedFitTargetId = useMemo(() => getMatchingFitTargetId({ viewport, targets: fitTargets }), [fitTargets, viewport])
  const isFullTimelineVisible = displayedFitTargetId === 'all'

  // Fit command - applies the canonical target range selected by the toolbar.
  const fitTarget = useCallback(
    (targetId) => {
      const target = fitTargets.find((candidate) => candidate.id === targetId)
      if (target) {
        setViewport(target.viewport)
      }
    },
    [fitTargets],
  )

  // Reset command - toolbar reset always returns to the latest full timeline duration.
  const resetView = useCallback(() => {
    setViewport(fitToFull(totalDurationRef.current, timelineMinimumRef.current))
  }, [])

  // Follow command - reuses playhead-follow behavior for active edge scrolling during a drag.
  const followSecond = useCallback((second, timelineMinimum) => {
    const previousViewport = viewportRef.current
    const nextViewport = followPlayhead({
      playheadSecond: second,
      timelineMinimum,
      viewStart: previousViewport.viewStart,
      viewEnd: previousViewport.viewEnd,
      totalDuration: totalDurationRef.current,
    })

    if (nextViewport.viewStart === previousViewport.viewStart && nextViewport.viewEnd === previousViewport.viewEnd) return null

    viewportRef.current = nextViewport
    setViewport(nextViewport)
    return {
      deltaStart: nextViewport.viewStart - previousViewport.viewStart,
      viewport: nextViewport,
    }
  }, [])

  // Zoom command - pivots around the playhead or wheel pointer while preserving timeline bounds.
  const zoomBy = useCallback(
    (direction, pivot) => {
      setViewport((previousViewport) =>
        zoomRange({
          viewStart: previousViewport.viewStart,
          viewEnd: previousViewport.viewEnd,
          pivot,
          direction,
          timelineMinimum: timelineMinimumRef.current,
          totalDuration: totalDurationRef.current,
          widthPx,
        }),
      )
    },
    [widthPx],
  )

  const zoomOut = useCallback(() => {
    zoomBy(-1, playheadSecond)
  }, [playheadSecond, zoomBy])

  const zoomIn = useCallback(() => {
    zoomBy(1, playheadSecond)
  }, [playheadSecond, zoomBy])

  // Pan command - lane-background drag passes seconds, while pure helpers enforce clamping.
  const panBy = useCallback((deltaSeconds) => {
    setViewport((previousViewport) =>
      panViewport({
        viewStart: previousViewport.viewStart,
        viewEnd: previousViewport.viewEnd,
        deltaSeconds,
        timelineMinimum: timelineMinimumRef.current,
        totalDuration: totalDurationRef.current,
      }),
    )
  }, [])

  // Wheel navigation - scroll zooms around the pointer; Ctrl+scroll pans the visible timeline.
  const handleWheel = useCallback(
    (event) => {
      event.preventDefault()
      const rect = containerElement?.getBoundingClientRect()
      if (!rect) return

      if (event.ctrlKey) {
        const span = viewport.viewEnd - viewport.viewStart
        const deltaSeconds = rect.width > 0 ? (event.deltaY / rect.width) * span : 0
        panBy(deltaSeconds)
        return
      }

      const pivot = viewport.viewStart + ((event.clientX - rect.left) / rect.width) * (viewport.viewEnd - viewport.viewStart)
      zoomBy(event.deltaY < 0 ? 1 : -1, pivot)
    },
    [containerElement, panBy, viewport, zoomBy],
  )

  // Tick model - presentational components receive rounded pixel positions and percent label positions.
  const ticks = useMemo(() => {
    const rawTicks = computeTimelineTicks({ viewStart: viewport.viewStart, viewEnd: viewport.viewEnd, widthPx })
    const pixelRatio = getDevicePixelRatio()

    return {
      major: rawTicks.major.map((tick, index) => ({
        id: `major-${index}`,
        label: tick.label,
        lineStyle: { left: roundToDevicePixel(tick.x, pixelRatio) },
        labelStyle: { left: getLeftPercent(tick.x, widthPx) },
      })),
      minor: rawTicks.minor.map((tick, index) => ({
        id: `minor-${index}`,
        lineStyle: { left: roundToDevicePixel(tick.x, pixelRatio) },
      })),
    }
  }, [viewport.viewEnd, viewport.viewStart, widthPx])

  // Viewport API - returns render-ready geometry plus commands for toolbar and gesture hooks.
  return {
    containerElement,
    containerRef,
    displayedFitTargetId,
    fitTarget,
    fitTargets,
    followSecond,
    handleWheel,
    isFullTimelineVisible,
    panBy,
    resetView,
    ticks,
    timelineMinimum,
    viewport,
    widthPx,
    zoomIn,
    zoomOut,
  }
}
