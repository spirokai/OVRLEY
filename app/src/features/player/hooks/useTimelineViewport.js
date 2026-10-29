/**
 * Owns timeline viewport state, DOM measurement, fit targets, ticks, zoom, pan, and auto-follow.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { roundToDevicePixel } from '../utils/timelineGeometry'
import { getTimelineMinimum } from '../utils/playerTiming'
import {
  buildFitTargets,
  clampToView,
  computeTimelineTickStrip,
  fitToFull,
  followPlayhead,
  getMatchingFitTargetId,
  getTimelineTickStep,
  panViewport,
  zoomRange,
} from '../utils/timelineViewport'

const PX_PER_SECOND_PRECISION = 1e6

function getDevicePixelRatio() {
  if (typeof window === 'undefined') return 1
  return window.devicePixelRatio || 1
}

// Screen pixels per layout pixel, including ancestor scale transforms such as the app shell's UI scale,
// plus the container's screen x. Snapping with devicePixelRatio alone lands on fractional screen pixels whenever
// the shell is scaled or the container starts between screen pixels.
function measureScreenPixelGrid(element) {
  const layoutWidth = element.offsetWidth
  if (layoutWidth === 0) return { ratio: getDevicePixelRatio(), origin: 0 } // Hidden timelines have no measurable scale.
  const rect = element.getBoundingClientRect()
  const devicePixelRatio = getDevicePixelRatio()
  return { ratio: (rect.width / layoutWidth) * devicePixelRatio, origin: rect.left * devicePixelRatio }
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
  // Playhead ref - zoom commands pivot on the latest playhead without changing identity every playback frame.
  const playheadSecondRef = useRef(playheadSecond)
  const [containerElement, setContainerElement] = useState(null)
  const [widthPx, setWidthPx] = useState(0)
  const [pixelGrid, setPixelGrid] = useState(() => ({ ratio: getDevicePixelRatio(), origin: 0 }))
  const viewport = useStore(useShallow((state) => state.timelineViewport))
  const setTimelineViewport = useStore((state) => state.setTimelineViewport)
  const setViewport = useCallback(
    (nextViewport) => {
      const current = useStore.getState().timelineViewport
      setTimelineViewport(typeof nextViewport === 'function' ? nextViewport(current) : nextViewport)
    },
    [setTimelineViewport],
  )
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

  useEffect(() => {
    playheadSecondRef.current = playheadSecond
  }, [playheadSecond])

  // Full-range reset - loading different media should never leave the user stranded in an old viewport.
  useEffect(() => {
    if (useStore.getState().skipNextTimelineViewportReset) {
      useStore.setState({ skipNextTimelineViewportReset: false })
      return
    }
    setViewport(fitToFull(totalDurationRef.current, timelineMinimumRef.current))
  }, [mediaIdentity, setViewport])

  // Duration changes can shorten the current viewport without changing its zoom level.
  useEffect(() => {
    if (isDragging) return
    setViewport((previousViewport) =>
      clampToView(previousViewport.viewStart, previousViewport.viewEnd, totalDurationRef.current, timelineMinimumRef.current),
    )
  }, [isDragging, setViewport, timelineMinimum, totalDuration])

  // Width measurement - immediate rect reads avoid invisible geometry before ResizeObserver fires.
  useEffect(() => {
    if (!containerElement) {
      setWidthPx(0)
      return undefined
    }

    const measureWidth = () => {
      const rect = containerElement.getBoundingClientRect?.()
      setWidthPx(rect?.width || 0)
      setPixelGrid(measureScreenPixelGrid(containerElement))
    }

    measureWidth()
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener?.('resize', measureWidth)
      return () => window.removeEventListener?.('resize', measureWidth)
    }
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setWidthPx(entry.contentRect.width || containerElement.getBoundingClientRect?.().width || 0)
        setPixelGrid(measureScreenPixelGrid(containerElement))
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
  }, [isDragging, isPlaying, playheadSecond, setViewport])

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
    [fitTargets, setViewport],
  )

  // Reset command - toolbar reset always returns to the latest full timeline duration.
  const resetView = useCallback(() => {
    setViewport(fitToFull(totalDurationRef.current, timelineMinimumRef.current))
  }, [setViewport])

  // Follow command - reuses playhead-follow behavior for active edge scrolling during a drag.
  const followSecond = useCallback(
    (second, timelineMinimum) => {
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
    },
    [setViewport],
  )

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
    [setViewport, widthPx],
  )

  const zoomOut = useCallback(() => {
    zoomBy(-1, playheadSecondRef.current)
  }, [zoomBy])

  const zoomIn = useCallback(() => {
    zoomBy(1, playheadSecondRef.current)
  }, [zoomBy])

  // Pan command - lane-background drag passes seconds, while pure helpers enforce clamping.
  const panBy = useCallback(
    (deltaSeconds) => {
      setViewport((previousViewport) =>
        panViewport({
          viewStart: previousViewport.viewStart,
          viewEnd: previousViewport.viewEnd,
          deltaSeconds,
          timelineMinimum: timelineMinimumRef.current,
          totalDuration: totalDurationRef.current,
        }),
      )
    },
    [setViewport],
  )

  // Wheel navigation - scroll zooms around the pointer; Ctrl+scroll pans the visible timeline.
  const handleWheel = useCallback(
    (event) => {
      const match = matchKeyboardShortcut(event, 'player')
      if (!match) return

      event.preventDefault()
      const rect = containerElement?.getBoundingClientRect()
      if (!rect) return

      if (match.commandId === 'timeline.wheelPan') {
        const span = viewport.viewEnd - viewport.viewStart
        const deltaSeconds = rect.width > 0 ? (event.deltaY / rect.width) * span : 0
        panBy(deltaSeconds)
        return
      }

      if (match.commandId !== 'timeline.wheelZoom') return

      const pivot = viewport.viewStart + ((event.clientX - rect.left) / rect.width) * (viewport.viewEnd - viewport.viewStart)
      zoomBy(event.deltaY < 0 ? 1 : -1, pivot)
    },
    [containerElement, panBy, viewport, zoomBy],
  )

  // Tick model - the strip is rebuilt only when zoom, width, or the major step under the viewport start changes.
  // Playback follow pans every frame, which then only moves the strip instead of restyling every tick.
  // Panning keeps the span but recomputes viewEnd - viewStart with float error; quantize so the strip memo stays hit.
  const pixelRatio = pixelGrid.ratio
  const span = viewport.viewEnd - viewport.viewStart
  const pxPerSecond = span > 0 && widthPx > 0 ? Math.round((widthPx / span) * PX_PER_SECOND_PRECISION) / PX_PER_SECOND_PRECISION : 0
  const tickStep = pxPerSecond > 0 ? getTimelineTickStep(pxPerSecond) : 0
  const tickAnchorSecond = tickStep > 0 ? Math.floor(viewport.viewStart / tickStep) * tickStep : 0
  const tickStrip = useMemo(() => {
    if (tickStep === 0) return { major: [], minor: [] } // Nothing to draw until the axis is measured.
    const rawTicks = computeTimelineTickStrip({ anchorSecond: tickAnchorSecond, step: tickStep, pxPerSecond, widthPx })

    return {
      major: rawTicks.major.map((tick, index) => ({
        id: `major-${index}`,
        label: tick.label,
        lineStyle: { left: roundToDevicePixel(tick.x, pixelRatio) },
        labelStyle: { left: tick.x },
      })),
      minor: rawTicks.minor.map((tick, index) => ({
        id: `minor-${index}`,
        lineStyle: { left: roundToDevicePixel(tick.x, pixelRatio) },
      })),
    }
  }, [pixelRatio, pxPerSecond, tickAnchorSecond, tickStep, widthPx])
  const ticks = useMemo(() => {
    const offset = roundToDevicePixel((tickAnchorSecond - viewport.viewStart) * pxPerSecond, pixelRatio)
    return { ...tickStrip, stripStyle: { transform: `translateX(${offset}px)` } }
  }, [pixelRatio, pxPerSecond, tickAnchorSecond, tickStrip, viewport.viewStart])

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
    pixelGrid,
    resetView,
    ticks,
    timelineMinimum,
    viewport,
    widthPx,
    zoomIn,
    zoomOut,
  }
}
