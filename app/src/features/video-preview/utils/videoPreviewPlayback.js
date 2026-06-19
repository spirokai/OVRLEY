/**
 * Pure helpers for syncing the preview <video> element.
 */

import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'

const HAVE_CURRENT_DATA = 2
const FIRST_FRAME_PRIME_SECOND = 0.001
const LAST_FRAME_PAINT_MARGIN_SECONDS = 0.001

/**
 * Returns the latest timestamp that still reliably has a drawable frame.
 *
 * WebKit-backed video elements can stop painting when currentTime is exactly
 * duration. Keeping the media element just inside the duration preserves the
 * visible final frame without changing the app's logical playhead.
 *
 * @param {HTMLVideoElement} video - The video element whose duration is used.
 * @returns {number|null} Last drawable time, or null when duration is unknown.
 */
export function getLastDrawableVideoSecond(video) {
  const duration = Number(video?.duration)

  if (!Number.isFinite(duration) || duration <= 0) {
    return null
  }

  return Math.max(0, duration - Math.min(LAST_FRAME_PAINT_MARGIN_SECONDS, duration))
}

/**
 * Clamps a requested video time into the element's valid range.
 *
 * @param {HTMLVideoElement} video - The video element whose duration is used as the upper bound.
 * @param {number} second - Requested time in seconds.
 * @returns {number} Clamped time in seconds.
 */
export function clampVideoTime(video, second) {
  const nextSecond = Number(second)

  if (!Number.isFinite(nextSecond) || nextSecond <= 0) {
    return 0
  }

  const duration = Number(video.duration)
  if (Number.isFinite(duration) && duration > 0) {
    if (nextSecond >= duration) {
      return getLastDrawableVideoSecond(video)
    }

    return Math.min(nextSecond, duration)
  }

  return nextSecond
}

/**
 * Applies a clamped currentTime update when the difference exceeds epsilon.
 *
 * @param {HTMLVideoElement} video - The video element to sync.
 * @param {number} second - Target time in seconds.
 * @param {number} [epsilonSeconds=0.001] - Minimum difference required before assigning.
 * @returns {void}
 */
export function syncVideoCurrentTime(video, second, epsilonSeconds = 0.001) {
  const safeSecond = clampVideoTime(video, second)

  if (Math.abs(video.currentTime - safeSecond) <= epsilonSeconds) {
    return
  }

  incrementPreviewPerfCounter(previewPerfCounterName('video.currentTime assignments'))
  video.currentTime = safeSecond
}

/**
 * Nudges a metadata-only paused video into decoding its first drawable frame.
 *
 * Some WebKit-backed video elements report metadata at t=0 without painting a
 * frame until playback starts. Seeking a tiny amount keeps the preview visually
 * at the beginning while forcing frame decode.
 *
 * @param {HTMLVideoElement} video - Preview video element.
 * @returns {boolean} Whether a priming seek was issued.
 */
export function primeVideoFirstFrame(video) {
  if (!video || video.readyState >= HAVE_CURRENT_DATA || video.currentTime > 0) {
    return false
  }

  const duration = Number(video.duration)
  if (!Number.isFinite(duration) || duration <= 0) {
    return false
  }

  video.currentTime = Math.min(FIRST_FRAME_PRIME_SECOND, duration)
  return true
}

/**
 * Produces a user-facing preview error from a native MediaError.
 *
 * @param {MediaError|null|undefined} error - Native video error object.
 * @returns {string} Preview error message.
 */
export function describeMediaError(error) {
  switch (error?.code) {
    case MediaError.MEDIA_ERR_ABORTED:
      return 'Video preview loading was aborted.'
    case MediaError.MEDIA_ERR_NETWORK:
      return 'The local preview server could not read the video file. The file may have been moved, deleted, or become unavailable.'
    case MediaError.MEDIA_ERR_DECODE:
      return 'The video could not be decoded by the system video player. This may happen with some HEVC, 10-bit, or 4:2:2 files.'
    case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
      return 'This video format is not supported by the system video player.'
    default:
      return 'The video preview could not be loaded.'
  }
}
