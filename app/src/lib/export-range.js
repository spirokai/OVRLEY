/**
 * Provides shared helpers for custom export range handling.
 */

import { clamp } from '@/lib/utils'
import { resolveActivityDuration } from '@/lib/preview-timing'

/**
 * Converts a time string (HH:MM:SS, MM:SS, or plain seconds) to seconds.
 * Empty/null input returns 0.
 *
 * @param {string|null|undefined} timeStr - Time string in HH:MM:SS or MM:SS format.
 * @returns {number} Total seconds.
 */
export function timeToSeconds(timeStr) {
  if (timeStr === null || timeStr === undefined || timeStr === '') {
    return 0
  }

  const str = String(timeStr).trim()
  if (str === '') return 0

  const isNegative = str.startsWith('-')
  const absStr = isNegative ? str.substring(1) : str

  if (absStr.includes(':')) {
    const parts = absStr.split(':')
    let seconds = 0
    if (parts.length === 3) {
      seconds = parseInt(parts[0], 10) * 3600 + parseInt(parts[1], 10) * 60 + parseFloat(parts[2])
    } else if (parts.length === 2) {
      seconds = parseInt(parts[0], 10) * 60 + parseFloat(parts[1])
    }
    return isNegative ? -seconds : seconds
  }

  const parsed = parseFloat(str)
  return isNaN(parsed) ? 0 : parsed
}

/**
 * Converts a numeric second value to HH:MM:SS.
 *
 * @param {number} seconds - Whole or fractional second value.
 * @returns {string} Zero-padded HH:MM:SS string.
 */
export function formatExportRangeTime(seconds) {
  const sign = seconds < 0 ? '-' : ''
  const safeSeconds = Math.trunc(Math.abs(seconds))
  const hours = Math.floor(safeSeconds / 3600)
  const minutes = Math.floor((safeSeconds % 3600) / 60)
  const remainingSeconds = safeSeconds % 60

  return `${sign}${[hours, minutes, remainingSeconds].map((part) => String(part).padStart(2, '0')).join(':')}`
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
  const duration = resolveActivityDuration({ sourceActivity: activity })

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
