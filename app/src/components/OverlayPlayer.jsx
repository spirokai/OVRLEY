/**
 * Renders the overlay player portion of the application interface.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Pause, Play, RotateCcw } from 'lucide-react'
import useStore from '@/store/useStore'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { getEffectivePreviewFps } from './overlay-editor/previewInterpolation'

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Checks whether is playback shortcut target.
 *
 * @param {*} target - Target object, element, or value being updated.
 * @returns {boolean} Whether the condition is satisfied.
 */
function isPlaybackShortcutTarget(target) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return Boolean(target.closest('input, textarea, select, button, a, [role="slider"], [contenteditable="true"]'))
}

/**
 * Formats timeline time.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {string} Formatted representation of the input.
 */
function formatTimelineTime(value) {
  const safeValue = Math.max(0, Math.floor(Number(value) || 0))
  const hours = Math.floor(safeValue / 3600)
  const minutes = Math.floor((safeValue % 3600) / 60)
  const seconds = safeValue % 60

  if (hours > 0) {
    return [hours, minutes, seconds].map((part, index) => String(part).padStart(index === 0 ? 1 : 2, '0')).join(':')
  }

  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
}

function resolvePlaybackSource({ shouldUseVideoPlayback, playheadSecond, videoSyncOffsetSeconds, importedVideoDuration }) {
  if (!shouldUseVideoPlayback) {
    return 'timeline'
  }

  const safePlayheadSecond = Number(playheadSecond) || 0
  const videoStartSecond = Math.max(0, Number(videoSyncOffsetSeconds) || 0)
  const safeVideoDuration = Number(importedVideoDuration)
  const hasVideoEnd = Number.isFinite(safeVideoDuration) && safeVideoDuration > 0
  const videoEndSecond = hasVideoEnd ? videoStartSecond + safeVideoDuration : Number.POSITIVE_INFINITY

  if (safePlayheadSecond < videoStartSecond || safePlayheadSecond >= videoEndSecond) {
    return 'timeline'
  }

  return 'video'
}

/**
 * Renders the overlay player component.
 *
 * @param {object} props - Component props.
 * @param {string} props.backgroundMode - Selected canvas background style.
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayPlayer({ backgroundMode }) {
  const activitySummary = useStore((state) => state.activitySummary)
  const sceneFps = useStore((state) => state.config?.scene?.fps ?? 30)
  const dummyDurationSeconds = useStore((state) => state.dummyDurationSeconds)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const updateRate = useStore((state) => state.updateRate)
  const setSelectedSecondTransient = useStore((state) => state.setSelectedSecondTransient)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const startPreviewPlayback = useStore((state) => state.startPreviewPlayback)
  const pausePreviewPlayback = useStore((state) => state.pausePreviewPlayback)
  const beginPreviewScrub = useStore((state) => state.beginPreviewScrub)
  const updatePreviewScrub = useStore((state) => state.updatePreviewScrub)
  const commitPreviewScrub = useStore((state) => state.commitPreviewScrub)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoDuration = useStore((state) => state.importedVideoDuration)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)
  const [dragSecond, setDragSecond] = useState(null)
  const playbackAnchorRef = useRef({
    startedAtMs: 0,
    startedSecond: 0,
  })
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)
  const pendingTimelineSecondRef = useRef(null)
  const timelineChangeFrameRef = useRef(0)

  const totalDuration = useMemo(() => {
    const metadataDuration = Number(activitySummary?.durationSeconds) || 0
    const fallbackDuration = Number(dummyDurationSeconds) || 0
    const videoEnd = importedVideoPath ? videoSyncOffsetSeconds + importedVideoDuration : 0

    return Math.max(metadataDuration, fallbackDuration, videoEnd, 0)
  }, [activitySummary?.durationSeconds, dummyDurationSeconds, importedVideoPath, videoSyncOffsetSeconds, importedVideoDuration])

  const hasActivity = Boolean(activitySummary && totalDuration > 0)
  const shouldUseVideoPlayback = backgroundMode === 'video' && Boolean(importedVideoPath)
  const isPlaying = previewPlaybackState === 'playing'
  const isTimelinePlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'timeline'
  const isVideoPlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'video'
  const clampedPlayhead = clamp(Number(selectedSecond) || 0, 0, totalDuration)
  const displayedPlayhead = clamp(dragSecond === null ? clampedPlayhead : dragSecond, 0, totalDuration)
  const effectivePreviewFps = useMemo(() => getEffectivePreviewFps(sceneFps, updateRate), [sceneFps, updateRate])

  const cancelPendingTimelineChange = useCallback(() => {
    if (timelineChangeFrameRef.current) {
      window.cancelAnimationFrame(timelineChangeFrameRef.current)
      timelineChangeFrameRef.current = 0
    }

    pendingTimelineSecondRef.current = null
  }, [])

  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  useEffect(
    () => () => {
      cancelPendingTimelineChange()
    },
    [cancelPendingTimelineChange],
  )

  useEffect(() => {
    if (!hasActivity) {
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: 0,
      }
      return
    }

    if (clampedPlayhead !== selectedSecond) {
      setSelectedSecondTransient(clampedPlayhead)
    }
  }, [clampedPlayhead, hasActivity, selectedSecond, setSelectedSecondTransient])

  useEffect(() => {
    if (isVideoPlaybackActive && !shouldUseVideoPlayback) {
      cancelPendingTimelineChange()
      setDragSecond(null)
      pausePreviewPlayback(clampedPlayhead)
    }
  }, [cancelPendingTimelineChange, clampedPlayhead, isVideoPlaybackActive, pausePreviewPlayback, shouldUseVideoPlayback])

  useEffect(() => {
    if (!isPlaying || !shouldUseVideoPlayback) {
      return
    }

    const nextSource = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: clampedPlayhead,
      videoSyncOffsetSeconds,
      importedVideoDuration,
    })

    if (nextSource === previewPlaybackSource) {
      return
    }

    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)

    if (nextSource === 'timeline') {
      playbackAnchorRef.current = {
        startedAtMs: performance.now(),
        startedSecond: clampedPlayhead,
      }
    } else {
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: clampedPlayhead,
      }
    }

    startPreviewPlayback({
      source: nextSource,
      second: clampedPlayhead,
    })
  }, [
    cancelPendingTimelineChange,
    clampedPlayhead,
    importedVideoDuration,
    isPlaying,
    previewPlaybackSource,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    videoSyncOffsetSeconds,
  ])

  useEffect(() => {
    if (!isTimelinePlaybackActive || !hasActivity) return undefined

    let animationFrameId = 0

    const tick = (now) => {
      const elapsedSeconds = (now - playbackAnchorRef.current.startedAtMs) / 1000
      const nextSecond = playbackAnchorRef.current.startedSecond + elapsedSeconds
      const safeDuration = totalDurationRef.current

      if (nextSecond >= safeDuration) {
        pausePreviewPlayback(safeDuration)
        playbackAnchorRef.current = {
          startedAtMs: 0,
          startedSecond: safeDuration,
        }
        previewFrameRef.current = -1
        return
      }

      const frameIndex = Math.floor(nextSecond * effectivePreviewFps)

      if (frameIndex !== previewFrameRef.current) {
        previewFrameRef.current = frameIndex
        setSelectedSecondTransient(clamp(frameIndex / effectivePreviewFps, 0, safeDuration))
      }

      animationFrameId = window.requestAnimationFrame(tick)
    }

    animationFrameId = window.requestAnimationFrame(tick)

    return () => window.cancelAnimationFrame(animationFrameId)
  }, [effectivePreviewFps, hasActivity, isTimelinePlaybackActive, pausePreviewPlayback, setSelectedSecondTransient])

  const handlePlay = () => {
    if (!hasActivity) return

    const initialSecond = clampedPlayhead >= totalDuration ? 0 : clampedPlayhead
    const nextSource = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: initialSecond,
      videoSyncOffsetSeconds,
      importedVideoDuration,
    })

    if (nextSource === 'timeline') {
      playbackAnchorRef.current = {
        startedAtMs: performance.now(),
        startedSecond: initialSecond,
      }
    } else {
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: initialSecond,
      }
    }

    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    startPreviewPlayback({
      source: nextSource,
      second: initialSecond,
    })
  }

  const handlePause = () => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: clampedPlayhead,
    }
    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    pausePreviewPlayback(clampedPlayhead)
  }

  const handleReset = () => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: 0,
    }
    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    pausePreviewPlayback(0)
  }

  const handleTimelineChange = ([nextValue]) => {
    const nextSecond = clamp(nextValue, 0, totalDuration)

    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: nextSecond,
    }
    cancelPendingTimelineChange()
    setDragSecond(nextSecond)
    previewFrameRef.current = -1

    if (previewPlaybackState !== 'scrubbing') {
      beginPreviewScrub(nextSecond)
      return
    }

    updatePreviewScrub(nextSecond)
  }

  const handleTimelineCommit = ([nextValue]) => {
    const nextSecond = clamp(nextValue, 0, totalDuration)
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: nextSecond,
    }
    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    commitPreviewScrub(nextSecond)
  }

  useEffect(() => {
    const handleKeyDown = (event) => {
      if (
        event.repeat ||
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        !hasActivity ||
        isPlaybackShortcutTarget(event.target)
      ) {
        return
      }

      if (event.code === 'ArrowLeft' || event.code === 'ArrowRight') {
        event.preventDefault()
        const direction = event.code === 'ArrowRight' ? 1 : -1
        const nextSecond = clamp(clampedPlayhead + direction, 0, totalDuration)

        playbackAnchorRef.current = {
          startedAtMs: 0,
          startedSecond: nextSecond,
        }
        previewFrameRef.current = -1
        cancelPendingTimelineChange()
        setDragSecond(null)
        pausePreviewPlayback(nextSecond)
        return
      }

      if (event.code !== 'Space' || !hasActivity) {
        return
      }

      event.preventDefault()

      if (isPlaying) {
        playbackAnchorRef.current = {
          startedAtMs: 0,
          startedSecond: clampedPlayhead,
        }
        previewFrameRef.current = -1
        cancelPendingTimelineChange()
        setDragSecond(null)
        pausePreviewPlayback(clampedPlayhead)
        return
      }

      const initialSecond = clampedPlayhead >= totalDuration ? 0 : clampedPlayhead
      const nextSource = resolvePlaybackSource({
        shouldUseVideoPlayback,
        playheadSecond: initialSecond,
        videoSyncOffsetSeconds,
        importedVideoDuration,
      })

      if (nextSource === 'timeline') {
        playbackAnchorRef.current = {
          startedAtMs: performance.now(),
          startedSecond: initialSecond,
        }
      } else {
        playbackAnchorRef.current = {
          startedAtMs: 0,
          startedSecond: initialSecond,
        }
      }

      previewFrameRef.current = -1
      cancelPendingTimelineChange()
      setDragSecond(null)
      startPreviewPlayback({
        source: nextSource,
        second: initialSecond,
      })
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    clampedPlayhead,
    cancelPendingTimelineChange,
    commitPreviewScrub,
    beginPreviewScrub,
    hasActivity,
    isPlaying,
    pausePreviewPlayback,
    importedVideoDuration,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    totalDuration,
    updatePreviewScrub,
    videoSyncOffsetSeconds,
  ])

  return (
    <div className={hasActivity ? 'shrink-0 border-border/70 bg-black/30 px-5 py-4 backdrop-blur-sm' : 'hidden'}>
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1.5 rounded-2xl border border-border/70 p-1 shadow-sm">
          <SimpleTooltip side="top" content="Play live preview">
            <Button
              type="button"
              size="icon-sm"
              variant={isPlaying ? 'secondary' : 'default'}
              className="rounded-xl"
              disabled={!hasActivity || isPlaying}
              onClick={handlePlay}
            >
              <Play className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
          <SimpleTooltip side="top" content="Pause playback">
            <Button type="button" size="icon-sm" variant="ghost" className="rounded-xl" disabled={!hasActivity || !isPlaying} onClick={handlePause}>
              <Pause className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
          <SimpleTooltip side="top" content="Reset to start">
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              className="rounded-xl"
              disabled={!hasActivity || clampedPlayhead <= 0}
              onClick={handleReset}
            >
              <RotateCcw className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
        </div>

        <div className="flex min-w-0 flex-1 items-center gap-3">
          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">{formatTimelineTime(displayedPlayhead)}</span>
          <div className="relative min-w-0 flex-1">
            <Slider
              min={0}
              max={Math.max(totalDuration, 1)}
              step={0.1}
              value={[displayedPlayhead]}
              disabled={!hasActivity}
              onValueChange={handleTimelineChange}
              onValueCommit={handleTimelineCommit}
              trackChildren={
                importedVideoPath &&
                totalDuration > 0 && (
                  <div
                    className="absolute inset-y-0 bg-accent"
                    style={{
                      left: `${Math.max(0, (videoSyncOffsetSeconds / totalDuration) * 100)}%`,
                      width: `${Math.min(100, (importedVideoDuration / totalDuration) * 100)}%`,
                    }}
                  />
                )
              }
            />
          </div>
          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">{formatTimelineTime(totalDuration)}</span>
        </div>
      </div>
    </div>
  )
}
