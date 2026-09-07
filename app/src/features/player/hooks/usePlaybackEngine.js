/**
 * Orchestrates playback state, scrub state, and timeline-driven animation frames.
 */

import { useCallback, useEffect, useMemo, useRef } from 'react'
import { getContainerFps } from '@/lib/update-rate'
import { clamp } from '@/lib/utils'
import {
  createPlaybackAnchor,
  getTimelineMinimum,
  getTimelinePlaybackSecond,
  getTotalPlaybackDuration,
  resolvePlaybackSource,
} from '../utils/playerTiming'

/**
 * Manages the player clock and exposes direct second-based playback commands.
 *
 * @param {object} options Playback engine inputs from the player store.
 * @param {object|null} options.activitySummary Imported activity summary metadata.
 * @param {string} options.backgroundMode Active preview background mode.
 * @param {function} options.beginPreviewScrub Store action for entering scrub mode.
 * @param {function} options.commitPreviewScrub Store action for committing scrub mode.
 * @param {number} options.fallbackDurationSeconds Fallback timeline duration without activity metadata.
 * @param {number|null} options.importedVideoDuration Imported video duration in seconds.
 * @param {string|null} options.importedVideoPath Imported video path when video is available.
 * @param {function} options.pausePreviewPlayback Store action for pausing preview playback.
 * @param {string} options.previewPlaybackSource Current playback clock owner.
 * @param {string} options.previewPlaybackState Current playback state.
 * @param {number} options.sceneFps Scene frames per second.
 * @param {number} options.selectedSecond Store playhead position.
 * @param {function} options.setSelectedSecond Store action for setting the playhead.
 * @param {function} options.startPreviewPlayback Store action for starting playback.
 * @param {function} options.updatePreviewScrub Store action for updating active scrub mode.
 * @param {number} options.updateRate Preview update-rate divisor.
 * @param {number} options.videoSyncOffsetSeconds Timeline second where video starts.
 * @returns {object} Playback state and direct second-based commands.
 */
export default function usePlaybackEngine({
  activitySummary,
  backgroundMode,
  beginPreviewScrub,
  commitPreviewScrub,
  fallbackDurationSeconds,
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
  // Imperative playback refs - RAF reads these without forcing React renders every frame.
  const playbackAnchorRef = useRef({ startedAtMs: 0, startedSecond: 0 })
  const playbackAnchorSourceRef = useRef(previewPlaybackSource)
  const playbackSecondRef = useRef(selectedSecond)
  const previousVideoPathRef = useRef(null)
  const scrubFrameRef = useRef(null)
  const latestScrubSecondRef = useRef(null)
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)

  // Duration derivation - activity, fallback templates, and imported video can each extend the playable range.
  const totalDuration = useMemo(
    () =>
      getTotalPlaybackDuration({
        activityDurationSeconds: activitySummary?.durationSeconds,
        fallbackDurationSeconds,
        importedVideoDuration,
        importedVideoPath,
        videoSyncOffsetSeconds,
      }),
    [activitySummary?.durationSeconds, fallbackDurationSeconds, importedVideoDuration, importedVideoPath, videoSyncOffsetSeconds],
  )

  const hasActivity = Boolean(activitySummary && totalDuration > 0)
  const shouldUseVideoPlayback = backgroundMode === 'video' && Boolean(importedVideoPath)
  const isPlaying = previewPlaybackState === 'playing'
  const isTimelinePlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'timeline'
  const timelineMinimum = getTimelineMinimum({ hasVideo: Boolean(importedVideoPath), videoSyncOffsetSeconds })
  const clampedPlayhead = clamp(selectedSecond, timelineMinimum, totalDuration)
  playbackSecondRef.current = clampedPlayhead
  const effectivePreviewFps = useMemo(() => getContainerFps(sceneFps, updateRate), [sceneFps, updateRate])

  const cancelScrub = useCallback(() => {
    latestScrubSecondRef.current = null
    if (scrubFrameRef.current === null) return

    window.cancelAnimationFrame(scrubFrameRef.current)
    scrubFrameRef.current = null
  }, [])

  const scheduleScrub = useCallback(
    (scrubSecond) => {
      latestScrubSecondRef.current = scrubSecond
      if (scrubFrameRef.current !== null) return

      scrubFrameRef.current = window.requestAnimationFrame(() => {
        scrubFrameRef.current = null
        const latestScrubSecond = latestScrubSecondRef.current
        latestScrubSecondRef.current = null

        if (latestScrubSecond === null) return
        if (previewPlaybackState !== 'scrubbing') {
          beginPreviewScrub(latestScrubSecond)
          return
        }
        updatePreviewScrub(latestScrubSecond)
      })
    },
    [beginPreviewScrub, previewPlaybackState, updatePreviewScrub],
  )

  useEffect(() => cancelScrub, [cancelScrub])

  // Shared reset path - any explicit playback command clears transient drag/frame ownership.
  const resetPlaybackOrchestration = useCallback(() => {
    previewFrameRef.current = -1
    cancelScrub()
  }, [cancelScrub])

  const setPlaybackAnchor = useCallback((source, second) => {
    playbackAnchorRef.current = createPlaybackAnchor({
      source,
      second,
      nowMs: performance.now(),
    })
    playbackAnchorSourceRef.current = source
  }, [])

  // Pause commands all anchor to the video clock because timeline wall-clock progression should stop.
  const pauseAtSecond = useCallback(
    (second) => {
      setPlaybackAnchor('video', second)
      resetPlaybackOrchestration()
      pausePreviewPlayback(second)
    },
    [pausePreviewPlayback, resetPlaybackOrchestration, setPlaybackAnchor],
  )

  // Duration sync - keeps the RAF loop current without changing paused playback ownership.
  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  // Video-import sync - a playhead at zero follows a newly imported negative video start, but later sync edits preserve it.
  useEffect(() => {
    const previousVideoPath = previousVideoPathRef.current
    previousVideoPathRef.current = importedVideoPath
    if (!importedVideoPath || importedVideoPath === previousVideoPath) return
    if (selectedSecond === 0 && timelineMinimum < 0) {
      setSelectedSecond(timelineMinimum)
    }
  }, [importedVideoPath, selectedSecond, setSelectedSecond, timelineMinimum])

  // Playhead bounds sync - clamps stale store values when media duration changes under the player.
  useEffect(() => {
    if (!hasActivity) {
      playbackAnchorRef.current = { startedAtMs: 0, startedSecond: timelineMinimum }
      playbackAnchorSourceRef.current = 'video'
      return
    }
    if (clampedPlayhead !== selectedSecond) {
      setSelectedSecond(clampedPlayhead)
    }
  }, [clampedPlayhead, hasActivity, selectedSecond, setSelectedSecond, timelineMinimum])

  // Video availability guard - if video playback is no longer possible, hand ownership back to a paused state.
  useEffect(() => {
    if (previewPlaybackSource !== 'video' || shouldUseVideoPlayback) {
      return
    }
    resetPlaybackOrchestration()
    setPlaybackAnchor('video', clampedPlayhead)
    pausePreviewPlayback(clampedPlayhead)
  }, [clampedPlayhead, pausePreviewPlayback, previewPlaybackSource, resetPlaybackOrchestration, setPlaybackAnchor, shouldUseVideoPlayback])

  // Clock handoff - timeline playback enters the video region; the video clock owns its ended handoff back to the timeline.
  useEffect(() => {
    if (!isPlaying || !shouldUseVideoPlayback || previewPlaybackSource !== 'timeline') {
      return
    }
    const nextSource = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: clampedPlayhead,
      videoSyncOffsetSeconds,
      importedVideoDuration,
    })
    if (nextSource !== 'video') {
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

  // Timeline clock - advances the store playhead from RAF when the video element is not the active clock.
  useEffect(() => {
    if (!isTimelinePlaybackActive || !hasActivity) {
      return undefined
    }
    if (playbackAnchorSourceRef.current !== 'timeline') {
      setPlaybackAnchor('timeline', playbackSecondRef.current)
    }
    let animationFrameId = 0
    const tick = (now) => {
      const timelineSecond = getTimelinePlaybackSecond({
        anchor: playbackAnchorRef.current,
        nowMs: now,
      })
      const safeDuration = totalDurationRef.current
      if (timelineSecond >= safeDuration) {
        pausePreviewPlayback(safeDuration)
        playbackAnchorRef.current = createPlaybackAnchor({
          source: 'video',
          second: safeDuration,
          nowMs: now,
        })
        playbackAnchorSourceRef.current = 'video'
        previewFrameRef.current = -1
        return
      }
      const frameIndex = Math.floor((timelineSecond - timelineMinimum) * effectivePreviewFps)
      if (frameIndex !== previewFrameRef.current) {
        previewFrameRef.current = frameIndex
        const frameSecond = Math.max(playbackAnchorRef.current.startedSecond, timelineMinimum + frameIndex / effectivePreviewFps)
        setSelectedSecond(clamp(frameSecond, timelineMinimum, safeDuration))
      }
      animationFrameId = window.requestAnimationFrame(tick)
    }
    animationFrameId = window.requestAnimationFrame(tick)
    return () => window.cancelAnimationFrame(animationFrameId)
  }, [effectivePreviewFps, hasActivity, isTimelinePlaybackActive, pausePreviewPlayback, setPlaybackAnchor, setSelectedSecond, timelineMinimum])

  // Play command - restart from the timeline start at the end and otherwise resume the current playhead.
  const play = useCallback(() => {
    if (!hasActivity) {
      return
    }
    let initialSecond = clampedPlayhead
    if (clampedPlayhead >= totalDuration) initialSecond = timelineMinimum
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
    timelineMinimum,
    videoSyncOffsetSeconds,
  ])

  // Transport commands - all non-play actions resolve to a paused playhead position.
  const pause = useCallback(() => {
    pauseAtSecond(clampedPlayhead)
  }, [clampedPlayhead, pauseAtSecond])

  const resetToStart = useCallback(() => {
    pauseAtSecond(timelineMinimum)
  }, [pauseAtSecond, timelineMinimum])

  const stepBySeconds = useCallback(
    (deltaSeconds) => {
      const targetSecond = clamp(clampedPlayhead + deltaSeconds, timelineMinimum, totalDuration)
      pauseAtSecond(targetSecond)
    },
    [clampedPlayhead, pauseAtSecond, timelineMinimum, totalDuration],
  )

  const scrubTo = useCallback(
    (second) => {
      // Scrub preview - retain only the latest pointer sample until the next animation frame.
      const scrubSecond = clamp(second, timelineMinimum, totalDuration)
      setPlaybackAnchor('video', scrubSecond)
      previewFrameRef.current = -1
      scheduleScrub(scrubSecond)
    },
    [scheduleScrub, setPlaybackAnchor, timelineMinimum, totalDuration],
  )

  const commitScrub = useCallback(
    (second) => {
      // Scrub commit - cancels queued preview work and stores the final paused playhead synchronously.
      const scrubSecond = clamp(second, timelineMinimum, totalDuration)
      cancelScrub()
      setPlaybackAnchor('video', scrubSecond)
      previewFrameRef.current = -1
      commitPreviewScrub(scrubSecond)
    },
    [cancelScrub, commitPreviewScrub, setPlaybackAnchor, timelineMinimum, totalDuration],
  )

  const jumpToEnd = useCallback(() => {
    pauseAtSecond(totalDuration)
  }, [pauseAtSecond, totalDuration])

  return {
    clampedPlayhead,
    cancelScrub,
    commitScrub,
    hasActivity,
    importedVideoDuration,
    importedVideoPath,
    isPlaying,
    jumpToEnd,
    pause,
    play,
    resetToStart,
    scrubTo,
    stepBySeconds,
    totalDuration,
    timelineMinimum,
    videoSyncOffsetSeconds,
  }
}
