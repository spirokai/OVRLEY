/**
 * Trim of the external activity: the part an overlay export renders when no
 * video decides it. Seconds are activity seconds, which are timeline seconds.
 */

import { clamp } from '@/lib/utils'

/** Shortest trimmed activity, in seconds. */
export const MIN_ACTIVITY_TRIM_SECONDS = 1

/**
 * @param {number} durationSeconds Activity duration.
 * @param {{startSecond: number, endSecond: number}|null} trim Activity trim, or null for the whole activity.
 * @returns {{startSecond: number, endSecond: number, isTrimmed: boolean}} Rendered activity window.
 */
export function resolveActivityWindow(durationSeconds, trim) {
  if (trim === null) return { startSecond: 0, endSecond: durationSeconds, isTrimmed: false }
  return { startSecond: trim.startSecond, endSecond: trim.endSecond, isTrimmed: true }
}

/**
 * Moves one edge of the trim, keeping it inside the activity and the edges
 * {@link MIN_ACTIVITY_TRIM_SECONDS} apart.
 * @param {{startSecond: number, endSecond: number}|null} trim Current trim, or null for the whole activity.
 * @param {number} durationSeconds Activity duration.
 * @param {'start'|'end'} edge Moved edge.
 * @param {number} second Requested activity second.
 * @returns {{startSecond: number, endSecond: number}|null} Next trim, or null when it covers the whole activity.
 */
export function trimActivityEdge(trim, durationSeconds, edge, second) {
  const { startSecond, endSecond } = resolveActivityWindow(durationSeconds, trim)
  const next =
    edge === 'start'
      ? { startSecond: clamp(second, 0, Math.max(0, endSecond - MIN_ACTIVITY_TRIM_SECONDS)), endSecond }
      : { startSecond, endSecond: clamp(second, Math.min(durationSeconds, startSecond + MIN_ACTIVITY_TRIM_SECONDS), durationSeconds) }
  return next.startSecond <= 0 && next.endSecond >= durationSeconds ? null : next
}
