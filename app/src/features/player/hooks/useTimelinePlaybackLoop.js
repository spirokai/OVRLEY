/**
 * Runs the timeline-backed preview playback loop.
 */

import { useEffect } from 'react'
import { clamp, createPlaybackAnchor, getTimelinePlaybackSecond } from '../utils/playerTimeline'

/**
 * Publishes preview seconds while the timeline clock owns playback.
 *
 * @param {object} options - Playback loop inputs.
 * @param {number} options.effectivePreviewFps - Effective preview frame rate.
 * @param {boolean} options.hasActivity - Whether the timeline has playable content.
 * @param {boolean} options.isTimelinePlaybackActive - Whether the timeline clock is active.
 * @param {function} options.pausePreviewPlayback - Store action that pauses playback.
 * @param {React.MutableRefObject<{ startedAtMs: number, startedSecond: number }>} options.playbackAnchorRef - Playback anchor ref.
 * @param {React.MutableRefObject<number>} options.previewFrameRef - Last published preview frame index.
 * @param {function} options.setSelectedSecond - Store action for transient playhead updates.
 * @param {React.MutableRefObject<number>} options.totalDurationRef - Total playable duration ref.
 * @returns {void}
 */
export function useTimelinePlaybackLoop({
  effectivePreviewFps,
  hasActivity,
  isTimelinePlaybackActive,
  pausePreviewPlayback,
  playbackAnchorRef,
  previewFrameRef,
  setSelectedSecond,
  totalDurationRef,
}) {
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
}
