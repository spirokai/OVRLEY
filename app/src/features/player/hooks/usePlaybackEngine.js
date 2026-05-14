/**
 * Playback engine hook for timeline and video-backed preview playback.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getEffectivePreviewFps } from '@/components/overlay-editor/previewInterpolation'
import { clamp, resolvePlaybackSource } from '../utils/playerTimeline'

/**
 * Orchestrates playback state, timeline scrubbing, and timeline-driven RAF playback.
 *
 * @param {object} options - Playback engine inputs.
 * @param {object|null} options.activitySummary - Imported activity summary metadata.
 * @param {string} options.backgroundMode - Selected canvas background style.
 * @param {function} options.beginPreviewScrub - Store action to enter scrub mode.
 * @param {function} options.commitPreviewScrub - Store action to commit a scrub.
 * @param {number} options.dummyDurationSeconds - Fallback duration used without activity metadata.
 * @param {number} options.importedVideoDuration - Imported video duration in seconds.
 * @param {string|null} options.importedVideoPath - Imported video path when a video exists.
 * @param {function} options.pausePreviewPlayback - Store action to pause playback.
 * @param {string} options.previewPlaybackSource - Active playback source.
 * @param {string} options.previewPlaybackState - Active playback state.
 * @param {number} options.sceneFps - Scene frames per second.
 * @param {number} options.selectedSecond - Current selected timeline second.
 * @param {function} options.setSelectedSecondTransient - Store action for transient playhead updates.
 * @param {function} options.startPreviewPlayback - Store action to start playback.
 * @param {function} options.updatePreviewScrub - Store action to update an active scrub.
 * @param {number} options.updateRate - Preview update rate.
 * @param {number} options.videoSyncOffsetSeconds - Timeline second where imported video starts.
 * @returns {object} Playback state and handlers used by the player UI and keyboard hook.
 */
export default function usePlaybackEngine({
  activitySummary,
  backgroundMode,
  beginPreviewScrub,
  commitPreviewScrub,
  dummyDurationSeconds,
  importedVideoDuration,
  importedVideoPath,
  pausePreviewPlayback,
  previewPlaybackSource,
  previewPlaybackState,
  sceneFps,
  selectedSecond,
  setSelectedSecondTransient,
  startPreviewPlayback,
  updatePreviewScrub,
  updateRate,
  videoSyncOffsetSeconds,
}) {
  // Local playback state - dragSecond temporarily owns the rendered playhead while the slider is being scrubbed
  const [dragSecond, setDragSecond] = useState(null)

  // Playback anchors - timeline playback is computed from a fixed start time and start second instead of accumulating deltas
  const playbackAnchorRef = useRef({
    startedAtMs: 0,
    startedSecond: 0,
  })

  // Imperative playback refs - keep high-frequency playback bookkeeping outside React render state
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)
  const pendingTimelineSecondRef = useRef(null)
  const timelineChangeFrameRef = useRef(0)

  // Derived timeline duration - activity metadata, dummy templates, and imported video can each extend the playable range
  const totalDuration = useMemo(() => {
    const metadataDuration = Number(activitySummary?.durationSeconds) || 0
    const fallbackDuration = Number(dummyDurationSeconds) || 0
    const videoEnd = importedVideoPath ? videoSyncOffsetSeconds + importedVideoDuration : 0

    return Math.max(metadataDuration, fallbackDuration, videoEnd, 0)
  }, [activitySummary?.durationSeconds, dummyDurationSeconds, importedVideoPath, videoSyncOffsetSeconds, importedVideoDuration])

  // Derived playback state - these values describe the active clock and the playhead shown by the UI
  const hasActivity = Boolean(activitySummary && totalDuration > 0)
  const shouldUseVideoPlayback = backgroundMode === 'video' && Boolean(importedVideoPath)
  const isPlaying = previewPlaybackState === 'playing'
  const isTimelinePlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'timeline'
  const isVideoPlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'video'
  const clampedPlayhead = clamp(Number(selectedSecond) || 0, 0, totalDuration)
  const displayedPlayhead = clamp(dragSecond === null ? clampedPlayhead : dragSecond, 0, totalDuration)
  const effectivePreviewFps = useMemo(() => getEffectivePreviewFps(sceneFps, updateRate), [sceneFps, updateRate])

  // Pending timeline cleanup - clears queued timeline work before playback mode changes, scrubs, or unmount
  const cancelPendingTimelineChange = useCallback(() => {
    if (timelineChangeFrameRef.current) {
      window.cancelAnimationFrame(timelineChangeFrameRef.current)
      timelineChangeFrameRef.current = 0
    }

    pendingTimelineSecondRef.current = null
  }, [])

  // Duration ref sync - RAF callbacks read the latest duration without resubscribing the animation loop every frame
  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  // Unmount cleanup - prevents delayed timeline updates from firing after the player leaves the tree
  useEffect(
    () => () => {
      cancelPendingTimelineChange()
    },
    [cancelPendingTimelineChange],
  )

  // Playhead bounds sync - clamps persisted or externally changed selectedSecond into the current timeline duration
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

  // Video playback availability sync - falls back to paused timeline playback if the active video clock disappears
  useEffect(() => {
    if (isVideoPlaybackActive && !shouldUseVideoPlayback) {
      cancelPendingTimelineChange()
      setDragSecond(null)
      pausePreviewPlayback(clampedPlayhead)
    }
  }, [cancelPendingTimelineChange, clampedPlayhead, isVideoPlaybackActive, pausePreviewPlayback, shouldUseVideoPlayback])

  // Playback source handoff - switches between timeline and video clocks when the playhead crosses video bounds
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

    // Timeline playback needs a fresh wall-clock anchor; video playback is driven by the video element clock.
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

  // Timeline playback loop - advances selectedSecond on animation frames while timeline playback is active
  useEffect(() => {
    if (!isTimelinePlaybackActive || !hasActivity) return undefined

    let animationFrameId = 0

    const tick = (now) => {
      const elapsedSeconds = (now - playbackAnchorRef.current.startedAtMs) / 1000
      const nextSecond = playbackAnchorRef.current.startedSecond + elapsedSeconds
      const safeDuration = totalDurationRef.current

      // Reaching the end commits a paused playhead at the exact duration and stops scheduling RAF ticks.
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

      // Publish only when the effective preview frame changes to avoid excessive transient store updates.
      if (frameIndex !== previewFrameRef.current) {
        previewFrameRef.current = frameIndex
        setSelectedSecondTransient(clamp(frameIndex / effectivePreviewFps, 0, safeDuration))
      }

      animationFrameId = window.requestAnimationFrame(tick)
    }

    animationFrameId = window.requestAnimationFrame(tick)

    return () => window.cancelAnimationFrame(animationFrameId)
  }, [effectivePreviewFps, hasActivity, isTimelinePlaybackActive, pausePreviewPlayback, setSelectedSecondTransient])

  // Playback control handlers - button and keyboard commands share these paths to keep behavior identical
  const handlePlay = useCallback(() => {
    if (!hasActivity) return

    const initialSecond = clampedPlayhead >= totalDuration ? 0 : clampedPlayhead
    const nextSource = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: initialSecond,
      videoSyncOffsetSeconds,
      importedVideoDuration,
    })

    // Starting from the timeline clock captures a wall-clock anchor; video starts from the requested second in store.
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
  }, [
    cancelPendingTimelineChange,
    clampedPlayhead,
    hasActivity,
    importedVideoDuration,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    totalDuration,
    videoSyncOffsetSeconds,
  ])

  const handlePause = useCallback(() => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: clampedPlayhead,
    }
    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    pausePreviewPlayback(clampedPlayhead)
  }, [cancelPendingTimelineChange, clampedPlayhead, pausePreviewPlayback])

  const handleReset = useCallback(() => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: 0,
    }
    previewFrameRef.current = -1
    cancelPendingTimelineChange()
    setDragSecond(null)
    pausePreviewPlayback(0)
  }, [cancelPendingTimelineChange, pausePreviewPlayback])

  const handleStep = useCallback(
    (direction) => {
      const nextSecond = clamp(clampedPlayhead + direction, 0, totalDuration)

      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: nextSecond,
      }
      previewFrameRef.current = -1
      cancelPendingTimelineChange()
      setDragSecond(null)
      pausePreviewPlayback(nextSecond)
    },
    [cancelPendingTimelineChange, clampedPlayhead, pausePreviewPlayback, totalDuration],
  )

  // Timeline scrub handlers - slider movement enters scrub state until commit stores the final paused playhead
  const handleTimelineChange = useCallback(
    ([nextValue]) => {
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
    },
    [beginPreviewScrub, cancelPendingTimelineChange, previewPlaybackState, totalDuration, updatePreviewScrub],
  )

  const handleTimelineCommit = useCallback(
    ([nextValue]) => {
      const nextSecond = clamp(nextValue, 0, totalDuration)
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: nextSecond,
      }
      previewFrameRef.current = -1
      cancelPendingTimelineChange()
      setDragSecond(null)
      commitPreviewScrub(nextSecond)
    },
    [cancelPendingTimelineChange, commitPreviewScrub, totalDuration],
  )

  return {
    clampedPlayhead,
    displayedPlayhead,
    handlePause,
    handlePlay,
    handleReset,
    handleStep,
    handleTimelineChange,
    handleTimelineCommit,
    hasActivity,
    importedVideoDuration,
    importedVideoPath,
    isPlaying,
    totalDuration,
    videoSyncOffsetSeconds,
  }
}
