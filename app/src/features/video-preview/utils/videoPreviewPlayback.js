/**
 * Pure helpers for syncing the preview <video> element.
 */

import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'

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
