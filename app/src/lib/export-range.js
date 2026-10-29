/**
 * Provides shared helpers for custom export range handling.
 */

import { clamp } from '@/lib/utils'
import { timeToSeconds } from '@/lib/time-format'

/**
 * Resolves the duration of optional external activity data for range controls.
 * Trimmed duration takes precedence over source metadata; absent or unusable
 * external durations leave an empty activity range.
 *
 * @param {object|null} activity Parsed activity data, or null without an activity.
 * @returns {number} Non-negative activity duration in seconds.
 */
export function resolveActivityDuration(activity) {
  const duration = Number(activity?.trim_end_seconds ?? activity?.metadata?.duration_seconds ?? 0)
  return Math.max(Number.isFinite(duration) ? duration : 0, 0)
}

/**
 * Applies a whole-second sidebar time string without allowing an inverted range.
 *
 * @param {{ type: string, from: number, to: number }} exportRange Current canonical range.
 * @param {'from'|'to'} boundary Boundary being edited.
 * @param {string} value Whole-second time string from the sidebar.
 * @returns {{ type: string, from: number, to: number }} Updated canonical range.
 */
export function setExportRangeBoundaryFromTimeInput(exportRange, boundary, value) {
  const second = timeToSeconds(value)
  if (boundary === 'from') {
    return { ...exportRange, from: second, to: Math.max(exportRange.to, second) }
  }
  return { ...exportRange, to: Math.max(second, exportRange.from) }
}

/**
 * Returns the range to use when enabling custom export range controls.
 * Existing valid custom bounds are preserved; empty/default 0-to-0 bounds are
 * expanded to cover the full activity range.
 *
 * @param {object} exportRange - Current export range config.
 * @param {number} activityEndSecond - Activity end in seconds.
 * @returns {{ type: string, from: number, to: number }}
 */
export function getCustomExportRangeDefault(exportRange, activityEndSecond) {
  if (exportRange.to > exportRange.from) {
    return {
      ...exportRange,
      type: 'custom',
    }
  }

  return {
    ...exportRange,
    type: 'custom',
    from: 0,
    to: activityEndSecond,
  }
}

/**
 * Resolves the effective export range window for widget scoping.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {object} exportRange - Export range config from store.
 * @returns {{ active: boolean, start: number, end: number }}
 */
export function resolveExportRangeWindow(activity, exportRange) {
  const duration = resolveActivityDuration(activity)

  if (exportRange.type !== 'custom') {
    return {
      active: false,
      start: 0,
      end: duration,
    }
  }

  const start = clamp(exportRange.from, 0, duration)
  const end = clamp(exportRange.to, 0, duration)

  if (!(end > start)) {
    return {
      active: false,
      start: 0,
      end: duration,
    }
  }

  return {
    active: true,
    start,
    end,
  }
}
