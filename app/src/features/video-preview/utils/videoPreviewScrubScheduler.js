/**
 * Coalesces rapid scrub seeks into timed currentTime updates.
 */

import { syncVideoCurrentTime } from './videoPreviewPlayback'

/**
 * Creates a scrub scheduler for a preview video element.
 *
 * @param {object} options - Scheduler inputs.
 * @param {number} options.epsilonSeconds - Seek epsilon passed to the video sync helper.
 * @param {number} options.flushIntervalMs - Minimum spacing between applied seeks.
 * @param {() => number} [options.getNowMs] - Clock source used for scheduling.
 * @param {HTMLVideoElement} options.video - Preview video element.
 * @returns {{ clear: () => void, schedule: (second: number) => void }} Scrub scheduling API.
 */
export function createVideoPreviewScrubScheduler({ epsilonSeconds, flushIntervalMs, getNowMs = () => performance.now(), video }) {
  let pendingSecond = null
  let pendingTimeoutId = null
  let lastSeekMs = 0

  const clear = () => {
    if (pendingTimeoutId !== null) {
      window.clearTimeout(pendingTimeoutId)
      pendingTimeoutId = null
    }

    pendingSecond = null
  }

  const flush = () => {
    pendingTimeoutId = null
    const nextSecond = pendingSecond
    pendingSecond = null
    lastSeekMs = getNowMs()

    if (nextSecond !== null) {
      syncVideoCurrentTime(video, nextSecond, epsilonSeconds)
    }
  }

  const schedule = (second) => {
    pendingSecond = second
    const elapsedMs = getNowMs() - lastSeekMs

    if (elapsedMs >= flushIntervalMs) {
      clear()
      pendingSecond = second
      flush()
      return
    }

    if (pendingTimeoutId === null) {
      pendingTimeoutId = window.setTimeout(flush, flushIntervalMs - elapsedMs)
    }
  }

  return {
    clear,
    schedule,
  }
}
