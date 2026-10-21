/**
 * Provides shared helpers for custom export range handling.
 */

import { clamp } from '@/lib/utils'
import { interpolateCoursePoint, coursePointsEqual, interpolateNumericSeries } from '@/lib/interpolation'
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
 * Applies one boundary of the export range without allowing an inverted range.
 *
 * @param {{ type: string, from: number, to: number }} exportRange Current canonical range.
 * @param {'from'|'to'} boundary Boundary being edited.
 * @param {number} second New boundary in timeline seconds.
 * @returns {{ type: string, from: number, to: number }} Updated canonical range.
 */
export function setExportRangeBoundary(exportRange, boundary, second) {
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
 * Returns the activity duration in seconds used by export-window helpers.
 *
 * @param {object|null} activity - Parsed activity data.
 * @returns {number} Duration in seconds.
 */
export function getActivityDurationSeconds(activity) {
  return resolveActivityDuration({ sourceActivity: activity })
}

/**
 * Resolves the effective export range window for widget scoping.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {object|null} exportRange - Export range config from store.
 * @param {boolean} [showFullActivity=false] - Whether to ignore the custom range.
 * @returns {{ active: boolean, duration: number, start: number, end: number }}
 */
export function resolveExportRangeWindow(activity, exportRange, showFullActivity = false) {
  const duration = getActivityDurationSeconds(activity)

  if (showFullActivity || exportRange?.type !== 'custom') {
    return {
      active: false,
      duration,
      start: 0,
      end: duration,
    }
  }

  const start = clamp(exportRange.from, 0, duration)
  const end = clamp(exportRange.to, 0, duration)

  if (!(end > start)) {
    return {
      active: false,
      duration,
      start: 0,
      end: duration,
    }
  }

  return {
    active: true,
    duration,
    start,
    end,
  }
}

/**
 * Interpolates the distance-progress values at the start and end of the
 * export window, returning the span metadata needed for rebasing progress.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @returns {{ start: number, end: number, span: number }|null} Distance span metadata.
 */
export function getExportWindowDistanceSpan(activity, window) {
  if (!window?.active) {
    return null
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  if (!elapsedSeries.length || !distanceProgress.length) {
    return null
  }

  const startProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, window.start)
  const endProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, window.end)

  if (!Number.isFinite(startProgress) || !Number.isFinite(endProgress)) {
    return null
  }

  const span = endProgress - startProgress
  if (!(span > 0)) {
    return null
  }

  return {
    start: clamp(startProgress, 0, 1),
    end: clamp(endProgress, 0, 1),
    span,
  }
}

/**
 * Rebases a global distance progress value to the export window's span.
 *
 * @param {number} value - Global distance progress (0–1).
 * @param {{ start: number, span: number }|null} distanceSpan - Distance span metadata.
 * @returns {number} Window-relative progress (0–1).
 */
export function normalizeDistanceProgressToWindow(value, distanceSpan) {
  if (!distanceSpan || !Number.isFinite(value) || !(distanceSpan.span > 0)) {
    return 0
  }

  return clamp((Number(value) - distanceSpan.start) / distanceSpan.span, 0, 1)
}

/**
 * Returns the current distance-based playback progress normalized within
 * the active export window at the given elapsed second.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @param {number} elapsedSecond - Current elapsed second.
 * @returns {number|null} Window-normalized progress (0–1) or null if not applicable.
 */
export function getWindowProgressAtTime(activity, window, elapsedSecond) {
  const distanceSpan = getExportWindowDistanceSpan(activity, window)
  if (!distanceSpan) {
    return null
  }

  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  const currentProgress = interpolateNumericSeries(elapsedSeries, distanceProgress, elapsedSecond)

  if (!Number.isFinite(currentProgress)) {
    return null
  }

  return normalizeDistanceProgressToWindow(currentProgress, distanceSpan)
}

/**
 * Builds route (course point) samples trimmed to the active export window.
 * Includes interpolated start/end points and rebased progress values.
 *
 * @param {object|null} activity - Parsed activity data.
 * @param {{ active: boolean, start: number, end: number }|null} window - Export window.
 * @returns {Array<{ point: number[], progress: number|null }>} Scoped route samples.
 */
export function buildExportWindowRouteSamples(activity, window) {
  const coursePoints = Array.isArray(activity?.sample_course_points) ? activity.sample_course_points : []
  const elapsedSeries = Array.isArray(activity?.sample_elapsed_seconds) ? activity.sample_elapsed_seconds : []
  const distanceProgress = Array.isArray(activity?.sample_distance_progress) ? activity.sample_distance_progress : []

  if (!window?.active || !elapsedSeries.length) {
    return coursePoints.reduce((result, point, index) => {
      if (!Array.isArray(point) || !Number.isFinite(point[0]) || !Number.isFinite(point[1])) {
        return result
      }

      result.push({
        point,
        progress: clamp(Number(distanceProgress[index]) || 0, 0, 1),
      })
      return result
    }, [])
  }

  const scopedSamples = []
  const distanceSpan = getExportWindowDistanceSpan(activity, window)
  const startPoint = interpolateCoursePoint(elapsedSeries, coursePoints, window.start)

  if (startPoint) {
    scopedSamples.push({
      point: startPoint,
      progress: distanceSpan ? 0 : null,
    })
  }

  for (let index = 0; index < elapsedSeries.length; index += 1) {
    const elapsed = Number(elapsedSeries[index])
    const point = coursePoints[index]
    if (
      !Number.isFinite(elapsed) ||
      elapsed <= window.start ||
      elapsed >= window.end ||
      !Array.isArray(point) ||
      !Number.isFinite(point[0]) ||
      !Number.isFinite(point[1])
    ) {
      continue
    }

    scopedSamples.push({
      point,
      progress: normalizeDistanceProgressToWindow(Number(distanceProgress[index]), distanceSpan),
    })
  }

  const endPoint = interpolateCoursePoint(elapsedSeries, coursePoints, window.end)
  if (endPoint) {
    const lastPoint = scopedSamples[scopedSamples.length - 1]?.point
    const lastProgress = scopedSamples[scopedSamples.length - 1]?.progress

    if (!coursePointsEqual(lastPoint, endPoint) || lastProgress !== 1) {
      scopedSamples.push({
        point: endPoint,
        progress: 1,
      })
    }
  }

  return scopedSamples
}

/**
 * The export range in the activity time widgets read: the custom range shifted by
 * the activity origin; without one, the telemetry clip's own duration, or the
 * interval covered by the videos.
 * @param {object} options Range inputs.
 * @param {{type: 'all'|'custom', from: number, to: number}} options.exportRange Timeline export range.
 * @param {number} options.activityOriginSecond Timeline second of activity second 0.
 * @param {number|null} options.activityClipDurationSeconds Duration of the clip supplying embedded telemetry, or null with an external activity.
 * @param {{startSecond: number, endSecond: number}|null} options.videoBounds Interval covered by the videos.
 * @returns {{type: 'all'|'custom', from: number, to: number}} Preview export range.
 */
export function resolvePreviewExportRange({ exportRange, activityOriginSecond, activityClipDurationSeconds, videoBounds }) {
  if (exportRange.type === 'custom') {
    return { ...exportRange, from: exportRange.from - activityOriginSecond, to: exportRange.to - activityOriginSecond }
  }
  if (activityClipDurationSeconds !== null) return { type: 'custom', from: 0, to: activityClipDurationSeconds }
  if (videoBounds === null) return exportRange
  return { type: 'custom', from: videoBounds.startSecond, to: videoBounds.endSecond }
}
