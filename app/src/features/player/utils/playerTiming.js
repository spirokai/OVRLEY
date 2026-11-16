/**
 * Pure playback and timeline timing helpers for the player feature.
 */

import { resolvePlaybackStart } from '@/lib/video-program/playback-target'

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
 * Resolves where preview playback starts and whether the timeline or the video
 * element drives it. Without video playback the timeline clock plays from the
 * requested second.
 *
 * @param {{ shouldUseVideoPlayback: boolean, playheadSecond: number, program: object }} options
 * @returns {{source: 'timeline'|'video', second: number}} Playback start.
 */
export function resolvePlaybackSource({ shouldUseVideoPlayback, playheadSecond, program }) {
  if (!shouldUseVideoPlayback) {
    return { source: 'timeline', second: playheadSecond }
  }

  return resolvePlaybackStart(program, playheadSecond)
}

/**
 * Resolves the first timeline second occupied by media.
 *
 * @param {{ videoBounds: {startSecond: number, endSecond: number}|null }} options
 * @returns {number} Timeline minimum for the active media.
 */
export function getTimelineMinimum({ videoBounds }) {
  return videoBounds === null ? 0 : Math.min(0, videoBounds.startSecond)
}

/**
 * Computes the largest playable duration across activity, template fallback,
 * and imported-video timing.
 *
 * @param {{ activityDurationSeconds: number|undefined, fallbackDurationSeconds: number, videoBounds: {startSecond: number, endSecond: number}|null }} options
 * @returns {number} Total playable duration in seconds.
 */
export function getTotalPlaybackDuration({ activityDurationSeconds, fallbackDurationSeconds, videoBounds }) {
  const activityDuration = activityDurationSeconds ?? 0
  const videoEnd = videoBounds === null ? 0 : videoBounds.endSecond

  const contentDuration = activityDuration > 0 ? activityDuration : videoBounds !== null ? 0 : fallbackDurationSeconds
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
