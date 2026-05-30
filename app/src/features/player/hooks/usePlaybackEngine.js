/**
 * Playback engine hook for timeline and video-backed preview playback.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getContainerFps } from '@/lib/update-rate'
import { clamp, createPlaybackAnchor, getTimelinePlaybackSecond, getTotalPlaybackDuration, resolvePlaybackSource } from '../utils/playerTimeline'

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
 * @param {function} options.setSelectedSecond - Store action for playhead updates.
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
  setSelectedSecond,
  startPreviewPlayback,
  updatePreviewScrub,
  updateRate,
  videoSyncOffsetSeconds,
}) {
  // Local playback state - dragSecond temporarily owns the rendered playhead while the slider is being scrubbed.
  const [dragSecond, setDragSecond] = useState(null)

  // Playback anchors - timeline playback uses a wall-clock anchor instead of accumulated deltas.
  const playbackAnchorRef = useRef({
    startedAtMs: 0,
    startedSecond: 0,
  })

  // Imperative refs - keep high-frequency frame bookkeeping out of React render state.
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)

  // Derived timeline duration - activity metadata, dummy templates, and imported video can each extend the playable range.
  const totalDuration = useMemo(
    () =>
      getTotalPlaybackDuration({
        activityDurationSeconds: activitySummary?.durationSeconds,
        dummyDurationSeconds,
        importedVideoDuration,
        importedVideoPath,
        videoSyncOffsetSeconds,
      }),
    [activitySummary?.durationSeconds, dummyDurationSeconds, importedVideoDuration, importedVideoPath, videoSyncOffsetSeconds],
  )

  // Derived playback state - describes the active clock and the playhead shown by the UI.
  const hasActivity = Boolean(activitySummary && totalDuration > 0)
  const shouldUseVideoPlayback = backgroundMode === 'video' && Boolean(importedVideoPath)
  const isPlaying = previewPlaybackState === 'playing'
  const isTimelinePlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'timeline'
  const clampedPlayhead = clamp(Number(selectedSecond) || 0, 0, totalDuration)
  const displayedPlayhead = clamp(dragSecond === null ? clampedPlayhead : dragSecond, 0, totalDuration)
  const effectivePreviewFps = useMemo(() => getContainerFps(sceneFps, updateRate), [sceneFps, updateRate])

  // Shared playback bookkeeping - clears transient UI ownership and resets frame dedup state.
  const resetPlaybackOrchestration = useCallback(() => {
    previewFrameRef.current = -1
    setDragSecond(null)
  }, [])

  // Playback anchors - timeline playback stores wall-clock time, while paused/video-backed states only keep the playhead.
  const setPlaybackAnchor = useCallback((source, second) => {
    playbackAnchorRef.current = createPlaybackAnchor({
      source,
      second,
      nowMs: performance.now(),
    })
  }, [])

  // Duration ref sync - RAF callbacks read the latest duration without resubscribing the animation loop every frame.
  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  // Playhead bounds sync - clamps persisted or externally changed selectedSecond into the current timeline duration.
  useEffect(() => {
    if (!hasActivity) {
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: 0,
      }
      return
    }

    if (clampedPlayhead !== selectedSecond) {
      setSelectedSecond(clampedPlayhead)
    }
  }, [clampedPlayhead, hasActivity, selectedSecond, setSelectedSecond])

  // Handles playback ownership between timeline and imported video clocks.
  useEffect(() => {
    if (previewPlaybackSource !== 'video' || shouldUseVideoPlayback) {
      return
    }

    resetPlaybackOrchestration()
    setPlaybackAnchor('video', clampedPlayhead)
    pausePreviewPlayback(clampedPlayhead)
  }, [clampedPlayhead, pausePreviewPlayback, previewPlaybackSource, resetPlaybackOrchestration, setPlaybackAnchor, shouldUseVideoPlayback])

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

    resetPlaybackOrchestration()
    setPlaybackAnchor(nextSource, clampedPlayhead)
    startPreviewPlayback({
      source: nextSource,
      second: clampedPlayhead,
    })
  }, [
    clampedPlayhead,
    importedVideoDuration,
    isPlaying,
    previewPlaybackSource,
    resetPlaybackOrchestration,
    setPlaybackAnchor,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    videoSyncOffsetSeconds,
  ])

  // Runs the timeline-backed preview playback loop via requestAnimationFrame.
  useEffect(() => {
    if (!isTimelinePlaybackActive || !hasActivity) {
      return undefined
    }

    let animationFrameId = 0

    const tick = (now) => {
      const nextSecond = getTimelinePlaybackSecond({
        anchor: playbackAnchorRef.current,
        nowMs: now,
      })
      const safeDuration = totalDurationRef.current

      if (nextSecond >= safeDuration) {
        pausePreviewPlayback(safeDuration)
        playbackAnchorRef.current = createPlaybackAnchor({
          source: 'video',
          second: safeDuration,
          nowMs: now,
        })
        previewFrameRef.current = -1
        return
      }

      const frameIndex = Math.floor(nextSecond * effectivePreviewFps)

      if (frameIndex !== previewFrameRef.current) {
        previewFrameRef.current = frameIndex
        setSelectedSecond(clamp(frameIndex / effectivePreviewFps, 0, safeDuration))
      }

      animationFrameId = window.requestAnimationFrame(tick)
    }

    animationFrameId = window.requestAnimationFrame(tick)

    return () => window.cancelAnimationFrame(animationFrameId)
  }, [
    effectivePreviewFps,
    hasActivity,
    isTimelinePlaybackActive,
    pausePreviewPlayback,
    playbackAnchorRef,
    previewFrameRef,
    setSelectedSecond,
    totalDurationRef,
  ])

  // Playback control handlers - button and keyboard commands share these paths to keep behavior identical.
  const handlePlay = useCallback(() => {
    if (!hasActivity) {
      return
    }

    const initialSecond = clampedPlayhead >= totalDuration ? 0 : clampedPlayhead
    const nextSource = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: initialSecond,
      videoSyncOffsetSeconds,
      importedVideoDuration,
    })

    setPlaybackAnchor(nextSource, initialSecond)
    resetPlaybackOrchestration()
    startPreviewPlayback({
      source: nextSource,
      second: initialSecond,
    })
  }, [
    clampedPlayhead,
    hasActivity,
    importedVideoDuration,
    resetPlaybackOrchestration,
    setPlaybackAnchor,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    totalDuration,
    videoSyncOffsetSeconds,
  ])

  const handlePause = useCallback(() => {
    setPlaybackAnchor('video', clampedPlayhead)
    resetPlaybackOrchestration()
    pausePreviewPlayback(clampedPlayhead)
  }, [clampedPlayhead, pausePreviewPlayback, resetPlaybackOrchestration, setPlaybackAnchor])

  const handleReset = useCallback(() => {
    setPlaybackAnchor('video', 0)
    resetPlaybackOrchestration()
    pausePreviewPlayback(0)
  }, [pausePreviewPlayback, resetPlaybackOrchestration, setPlaybackAnchor])

  const handleStep = useCallback(
    (direction) => {
      const nextSecond = clamp(clampedPlayhead + direction, 0, totalDuration)

      setPlaybackAnchor('video', nextSecond)
      resetPlaybackOrchestration()
      pausePreviewPlayback(nextSecond)
    },
    [clampedPlayhead, pausePreviewPlayback, resetPlaybackOrchestration, setPlaybackAnchor, totalDuration],
  )

  // Timeline scrub handlers - slider movement enters scrub state until commit stores the final paused playhead.
  const handleTimelineChange = useCallback(
    ([nextValue]) => {
      const nextSecond = clamp(nextValue, 0, totalDuration)

      setPlaybackAnchor('video', nextSecond)
      setDragSecond(nextSecond)
      previewFrameRef.current = -1

      if (previewPlaybackState !== 'scrubbing') {
        beginPreviewScrub(nextSecond)
        return
      }

      updatePreviewScrub(nextSecond)
    },
    [beginPreviewScrub, previewPlaybackState, setPlaybackAnchor, totalDuration, updatePreviewScrub],
  )

  const handleTimelineCommit = useCallback(
    ([nextValue]) => {
      const nextSecond = clamp(nextValue, 0, totalDuration)

      setPlaybackAnchor('video', nextSecond)
      previewFrameRef.current = -1
      setDragSecond(null)
      commitPreviewScrub(nextSecond)
    },
    [commitPreviewScrub, setPlaybackAnchor, totalDuration],
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
