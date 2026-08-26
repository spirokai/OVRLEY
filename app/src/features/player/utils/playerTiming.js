/**
 * Pure playback and timeline timing helpers for the player feature.
 */

/**
 * Snaps a timeline timestamp to the nearest source-video frame.
 *
 * @param {number} second Timeline timestamp in seconds.
 * @param {number} fps Imported video frame rate.
 * @param {number} [originSecond=0] Timeline timestamp of video frame zero.
 * @returns {number} Frame-aligned fractional timestamp.
 */
export function snapTimelineSecondToFrame(second, fps, originSecond = 0) {
  return originSecond + Math.round((second - originSecond) * fps) / fps
}

/**
 * Resolves whether preview playback should be driven by the timeline or video element.
 *
 * @param {{ shouldUseVideoPlayback: boolean, playheadSecond: number, videoSyncOffsetSeconds: number, importedVideoDuration: number }} options
 * @returns {'timeline'|'video'} Playback source for the current playhead.
 */
export function resolvePlaybackSource({ shouldUseVideoPlayback, playheadSecond, videoSyncOffsetSeconds, importedVideoDuration }) {
  if (!shouldUseVideoPlayback) {
    return 'timeline'
  }

  const videoEndSecond = videoSyncOffsetSeconds + importedVideoDuration

  if (playheadSecond < videoSyncOffsetSeconds || playheadSecond >= videoEndSecond) {
    return 'timeline'
  }

  return 'video'
}

/**
 * Resolves the first timeline second occupied by imported video.
 *
 * @param {{ hasVideo: boolean, videoSyncOffsetSeconds: number }} options
 * @returns {number} Timeline minimum for the active media.
 */
export function getTimelineMinimum({ hasVideo, videoSyncOffsetSeconds }) {
  return hasVideo ? Math.min(0, videoSyncOffsetSeconds) : 0
}

/**
 * Computes the largest playable duration across activity, template fallback,
 * and imported-video timing.
 *
 * @param {{ activityDurationSeconds: number, fallbackDurationSeconds: number, importedVideoDuration: number, importedVideoPath: string|null, videoSyncOffsetSeconds: number }} options
 * @returns {number} Total playable duration in seconds.
 */
export function getTotalPlaybackDuration({
  activityDurationSeconds,
  fallbackDurationSeconds,
  importedVideoDuration,
  importedVideoPath,
  videoSyncOffsetSeconds,
}) {
  const activityDuration = activityDurationSeconds ?? 0
  const videoEnd = importedVideoPath ? videoSyncOffsetSeconds + importedVideoDuration : 0

  const contentDuration = activityDuration > 0 ? activityDuration : importedVideoPath ? 0 : fallbackDurationSeconds
  return Math.max(contentDuration, videoEnd, 0)
}

/**
 * Builds a playback anchor for the active preview clock.
 *
 * @param {{ source: 'timeline'|'video', second: number, nowMs: number }} options
 * @returns {{ startedAtMs: number, startedSecond: number }} Playback anchor.
 */
export function createPlaybackAnchor({ source, second, nowMs }) {
  if (source === 'timeline') {
    return {
      startedAtMs: nowMs,
      startedSecond: second,
    }
  }

  return {
    startedAtMs: 0,
    startedSecond: second,
  }
}

/**
 * Resolves the elapsed timeline second from an active timeline anchor.
 *
 * @param {{ anchor: { startedAtMs: number, startedSecond: number }, nowMs: number }} options
 * @returns {number} Elapsed timeline second.
 */
export function getTimelinePlaybackSecond({ anchor, nowMs }) {
  return anchor.startedSecond + (nowMs - anchor.startedAtMs) / 1000
}
