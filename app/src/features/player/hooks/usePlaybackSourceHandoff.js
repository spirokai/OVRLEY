/**
 * Handles playback ownership between timeline and imported video clocks.
 */

import { useEffect } from 'react'
import { resolvePlaybackSource } from '../utils/playerTimeline'

/**
 * Keeps preview playback on the correct clock and hands ownership at video boundaries.
 *
 * @param {object} options - Handoff inputs.
 * @param {number} options.clampedPlayhead - Current in-range playhead.
 * @param {number} options.importedVideoDuration - Imported video duration.
 * @param {boolean} options.isPlaying - Whether preview playback is active.
 * @param {function} options.pausePreviewPlayback - Store action that pauses playback.
 * @param {string} options.previewPlaybackSource - Active playback source.
 * @param {function} options.resetPlaybackOrchestration - Clears transient playback bookkeeping.
 * @param {function} options.setPlaybackAnchor - Updates the current playback anchor.
 * @param {boolean} options.shouldUseVideoPlayback - Whether video-backed playback is available.
 * @param {function} options.startPreviewPlayback - Store action that starts playback.
 * @param {number} options.videoSyncOffsetSeconds - Timeline second where the video starts.
 * @returns {void}
 */
export function usePlaybackSourceHandoff({
  clampedPlayhead,
  importedVideoDuration,
  isPlaying,
  pausePreviewPlayback,
  previewPlaybackSource,
  resetPlaybackOrchestration,
  setPlaybackAnchor,
  shouldUseVideoPlayback,
  startPreviewPlayback,
  videoSyncOffsetSeconds,
}) {
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
}
