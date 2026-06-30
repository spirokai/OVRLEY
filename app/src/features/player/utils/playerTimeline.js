/**
 * Pure helpers for player timeline calculations and formatting.
 */

export { clamp } from '@/lib/utils'

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

/**
 * Computes the largest playable duration across activity, template fallback,
 * and imported-video timing.
 *
 * @param {object} options - Duration inputs.
 * @param {number} options.activityDurationSeconds - Activity-backed duration.
 * @param {number} options.dummyDurationSeconds - Fallback template duration.
 * @param {number} options.importedVideoDuration - Imported video duration.
 * @param {string|null} options.importedVideoPath - Imported video path when available.
 * @param {number} options.videoSyncOffsetSeconds - Timeline second where the video starts.
 * @returns {number} Total playable duration in seconds.
 */
export function getTotalPlaybackDuration({
  activityDurationSeconds,
  dummyDurationSeconds,
  importedVideoDuration,
  importedVideoPath,
  videoSyncOffsetSeconds,
}) {
  const metadataDuration = Number(activityDurationSeconds)
  const hasMetadataDuration = Number.isFinite(metadataDuration) && metadataDuration > 0
  const fallbackDuration = Number(dummyDurationSeconds) || 0
  const videoEnd = importedVideoPath ? (Number(videoSyncOffsetSeconds) || 0) + (Number(importedVideoDuration) || 0) : 0

  return Math.max(hasMetadataDuration ? metadataDuration : fallbackDuration, videoEnd, 0)
}

/**
 * Builds a playback anchor for the active preview clock.
 *
 * Timeline playback stores a wall-clock start time. Paused or video-backed
 * states only preserve the playhead second.
 *
 * @param {object} options - Anchor inputs.
 * @param {'timeline'|'video'} options.source - Playback source that owns the clock.
 * @param {number} options.second - Timeline second to anchor.
 * @param {number} options.nowMs - Current wall-clock time in milliseconds.
 * @returns {{ startedAtMs: number, startedSecond: number }} Playback anchor.
 */
export function createPlaybackAnchor({ source, second, nowMs }) {
  const safeSecond = Number(second) || 0

  if (source === 'timeline') {
    return {
      startedAtMs: nowMs,
      startedSecond: safeSecond,
    }
  }

  return {
    startedAtMs: 0,
    startedSecond: safeSecond,
  }
}

/**
 * Resolves the elapsed timeline second from an active timeline anchor.
 *
 * @param {object} options - Timeline playback inputs.
 * @param {{ startedAtMs: number, startedSecond: number }} options.anchor - Active timeline anchor.
 * @param {number} options.nowMs - Current wall-clock time in milliseconds.
 * @returns {number} Elapsed timeline second.
 */
export function getTimelinePlaybackSecond({ anchor, nowMs }) {
  return anchor.startedSecond + (nowMs - anchor.startedAtMs) / 1000
}
