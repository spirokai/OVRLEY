/**
 * Orchestrates playback state, scrub state, and timeline-driven animation frames.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from 'react'
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
 * @param {number} options.activityDurationSeconds External activity duration, or 0 without one.
 * @param {boolean} options.hasActivityData Whether external activity or any video telemetry drives widgets.
 * @param {string} options.backgroundMode Active preview background mode.
 * @param {function} options.beginPreviewScrub Store action for entering scrub mode.
 * @param {function} options.commitPreviewScrub Store action for committing scrub mode.
 * @param {number} options.fallbackDurationSeconds Fallback timeline duration without activity metadata.
 * @param {object} options.program Committed timeline program of the imported videos.
 * @param {{startSecond: number, endSecond: number}|null} options.videoBounds Committed interval covered by videos.
 * @param {string} options.videoIdentity Identity of the imported videos.
 * @param {function} options.pausePreviewPlayback Store action for pausing preview playback.
 * @param {string} options.previewPlaybackSource Current playback clock owner.
 * @param {string} options.previewPlaybackState Current playback state.
 * @param {number} options.sceneFps Scene frames per second.
 * @param {number} options.selectedSecond Store playhead position.
 * @param {function} options.setSelectedSecond Store action for setting the playhead.
 * @param {function} options.startPreviewPlayback Store action for starting playback.
 * @param {function} options.updatePreviewScrub Store action for updating active scrub mode.
 * @param {number} options.updateRate Preview update-rate divisor.
 * @returns {object} Playback state and direct second-based commands.
 */
export default function usePlaybackEngine({
  activityDurationSeconds,
  hasActivityData,
  backgroundMode,
  beginPreviewScrub,
  commitPreviewScrub,
  fallbackDurationSeconds,
  program,
  videoBounds,
  videoIdentity,
  pausePreviewPlayback,
  previewPlaybackSource,
  previewPlaybackState,
  sceneFps,
  selectedSecond,
  setSelectedSecond,
  startPreviewPlayback,
  updatePreviewScrub,
  updateRate,
}) {
  // Imperative playback refs - RAF reads these without forcing React renders every frame.
  const playbackAnchorRef = useRef({ startedAtMs: 0, startedSecond: 0 })
  const playbackAnchorSourceRef = useRef(previewPlaybackSource)
  const playbackSecondRef = useRef(selectedSecond)
  const previousVideoIdentityRef = useRef('')
  const scrubFrameRef = useRef(null)
  const latestScrubSecondRef = useRef(null)
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)

  // Duration derivation - activity, fallback templates, and imported video can each extend the playable range.
  const totalDuration = useMemo(
    () => getTotalPlaybackDuration({ activityDurationSeconds, fallbackDurationSeconds, videoBounds }),
    [activityDurationSeconds, fallbackDurationSeconds, videoBounds],
  )

  const hasActivity = hasActivityData && totalDuration > 0
  const shouldUseVideoPlayback = backgroundMode === 'video' && videoBounds !== null
  const isPlaying = previewPlaybackState === 'playing'
  const isTimelinePlaybackActive = previewPlaybackState === 'playing' && previewPlaybackSource === 'timeline'
  const timelineMinimum = getTimelineMinimum({ videoBounds })
  const clampedPlayhead = clamp(selectedSecond, timelineMinimum, totalDuration)
  const effectivePreviewFps = useMemo(() => getContainerFps(sceneFps, updateRate), [sceneFps, updateRate])

  useLayoutEffect(() => {
    playbackSecondRef.current = clampedPlayhead
  }, [clampedPlayhead])

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

  // Video-import sync - a playhead at zero follows newly imported videos that start before zero, but later sync edits preserve it.
  useEffect(() => {
    const previousVideoIdentity = previousVideoIdentityRef.current
    previousVideoIdentityRef.current = videoIdentity
    if (videoIdentity === '' || videoIdentity === previousVideoIdentity) return
    if (selectedSecond === 0 && timelineMinimum < 0) {
      setSelectedSecond(timelineMinimum)
    }
  }, [selectedSecond, setSelectedSecond, timelineMinimum, videoIdentity])

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

  // Clock handoff - timeline playback entering the program hands over to the video clock (skipping a gap
  // to the next piece); the video clock owns its piece-end handoffs.
  useEffect(() => {
    if (!isPlaying || !shouldUseVideoPlayback || previewPlaybackSource !== 'timeline') {
      return
    }
    const start = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: clampedPlayhead,
      program,
    })
    if (start.source !== 'video') {
      return
    }
    resetPlaybackOrchestration()
    setPlaybackAnchor(start.source, start.second)
    startPreviewPlayback(start)
  }, [
    clampedPlayhead,
    isPlaying,
    previewPlaybackSource,
    program,
    resetPlaybackOrchestration,
    setPlaybackAnchor,
    shouldUseVideoPlayback,
    startPreviewPlayback,
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

  // Play command - restart from the timeline start at the end and otherwise resume the current playhead
  // (from the next piece when the playhead is in a gap or deleted part).
  const play = useCallback(() => {
    if (!hasActivity) {
      return
    }
    let initialSecond = clampedPlayhead
    if (clampedPlayhead >= totalDuration) initialSecond = timelineMinimum
    const start = resolvePlaybackSource({
      shouldUseVideoPlayback,
      playheadSecond: initialSecond,
      program,
    })

    setPlaybackAnchor(start.source, start.second)
    resetPlaybackOrchestration()
    startPreviewPlayback(start)
  }, [
    clampedPlayhead,
    hasActivity,
    program,
    resetPlaybackOrchestration,
    setPlaybackAnchor,
    shouldUseVideoPlayback,
    startPreviewPlayback,
    totalDuration,
    timelineMinimum,
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
    isPlaying,
    jumpToEnd,
    pause,
    play,
    resetToStart,
    scrubTo,
    stepBySeconds,
    totalDuration,
    timelineMinimum,
  }
}
