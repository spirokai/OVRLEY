/**
 * Pure helpers for player timeline calculations and formatting.
 */

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {number} value - Input value to constrain.
 * @param {number} min - Lower bound used by the calculation.
 * @param {number} max - Upper bound used by the calculation.
 * @returns {number} Value constrained to the provided range.
 */
export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Formats a timeline second value as a clock label.
 *
 * @param {number} value - Timeline second value to format.
 * @returns {string} Timeline label in mm:ss or h:mm:ss format.
 */
export function formatTimelineTime(value) {
  const safeValue = Math.max(0, Math.floor(Number(value) || 0))
  const hours = Math.floor(safeValue / 3600)
  const minutes = Math.floor((safeValue % 3600) / 60)
  const seconds = safeValue % 60

  if (hours > 0) {
    return [hours, minutes, seconds].map((part, index) => String(part).padStart(index === 0 ? 1 : 2, '0')).join(':')
  }

  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
}

/**
 * Resolves whether preview playback should be driven by the timeline or video element.
 *
 * @param {object} options - Playback source inputs.
 * @param {boolean} options.shouldUseVideoPlayback - Whether video-backed playback is available.
 * @param {number} options.playheadSecond - Current timeline playhead second.
 * @param {number} options.videoSyncOffsetSeconds - Timeline second where the video starts.
 * @param {number} options.importedVideoDuration - Imported video duration in seconds.
 * @returns {'timeline'|'video'} Playback source for the current playhead.
 */
export function resolvePlaybackSource({ shouldUseVideoPlayback, playheadSecond, videoSyncOffsetSeconds, importedVideoDuration }) {
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
