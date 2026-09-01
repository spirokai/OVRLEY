/**
 * Pure viewport, range, and tick helpers for the player timeline.
 */

import { clamp } from '@/lib/utils'
import { formatClockDuration } from '@/lib/time-format'
import { getTimelineMinimum } from './playerTiming'
import { secondsToViewPx } from './timelineGeometry'
import i18next from 'i18next'

const VIEWPORT_MATCH_EPSILON_SECONDS = 0.001
const ZOOM_FACTOR = 1.6
const MIN_ZOOM_SPAN = 0.5
const FIT_MIN_SPAN = 2
const FIT_PADDING_RATIO = 0.04
const TICK_TARGET_PX = 90
const MAX_ZOOM_MAJOR_STEP_SECONDS = 2
const FOLLOW_EDGE_PADDING_RATIO = 0.15
const NICE_STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1200, 1500, 1800, 3600]

function getTimelineBounds(timelineMinimum, totalDuration) {
  return {
    start: timelineMinimum,
    end: totalDuration,
    span: totalDuration - timelineMinimum,
  }
}

/**
 * Compares two viewports with a small tolerance.
 *
 * @param {{ viewStart: number, viewEnd: number }|null} a First range.
 * @param {{ viewStart: number, viewEnd: number }|null} b Second range.
 * @returns {boolean} Whether both ranges are effectively equal.
 */
export function rangesMatch(a, b) {
  if (!a || !b) return false
  return Math.abs(a.viewStart - b.viewStart) <= VIEWPORT_MATCH_EPSILON_SECONDS && Math.abs(a.viewEnd - b.viewEnd) <= VIEWPORT_MATCH_EPSILON_SECONDS
}

/**
 * Clamps a viewport so it stays within [timelineMinimum, totalDuration].
 *
 * @param {number} viewStart Visible window start in seconds.
 * @param {number} viewEnd Visible window end in seconds.
 * @param {number} totalDuration Timeline end in seconds.
 * @param {number} [timelineMinimum=0] Timeline start in seconds.
 * @returns {{ viewStart: number, viewEnd: number }} Clamped viewport.
 */
export function clampToView(viewStart, viewEnd, totalDuration, timelineMinimum = 0) {
  const bounds = getTimelineBounds(timelineMinimum, totalDuration)
  const span = clamp(viewEnd - viewStart, 0, bounds.span)
  const clampedStart = clamp(viewStart, bounds.start, bounds.end - span)
  return { viewStart: clampedStart, viewEnd: clampedStart + span }
}

/**
 * Fits the viewport to the full playable range.
 *
 * @param {number} totalDuration Timeline end in seconds.
 * @param {number} [timelineMinimum=0] Timeline start in seconds.
 * @returns {{ viewStart: number, viewEnd: number }} Full-range viewport.
 */
export function fitToFull(totalDuration, timelineMinimum = 0) {
  return { viewStart: timelineMinimum, viewEnd: totalDuration }
}

function getMinimumZoomSpan(widthPx) {
  const safeWidth = Number(widthPx) || 0
  if (safeWidth <= 0) return MIN_ZOOM_SPAN
  return Math.max(MIN_ZOOM_SPAN, (safeWidth / TICK_TARGET_PX) * MAX_ZOOM_MAJOR_STEP_SECONDS)
}

/**
 * Zooms the viewport by a factor around a pivot point.
 *
 * @param {{ viewStart: number, viewEnd: number, pivot: number, direction: 1|-1, totalDuration: number, timelineMinimum?: number, widthPx?: number }} options
 * @returns {{ viewStart: number, viewEnd: number }} Zoomed viewport.
 */
export function zoomRange({ viewStart, viewEnd, pivot, direction, totalDuration, timelineMinimum = 0, widthPx = 0 }) {
  const bounds = getTimelineBounds(timelineMinimum, totalDuration)
  const span = viewEnd - viewStart
  const clampedPivot = clamp(pivot, viewStart, viewEnd)
  const ratio = span > 0 ? (clampedPivot - viewStart) / span : 0.5
  const minSpan = Math.min(getMinimumZoomSpan(widthPx), bounds.span)

  const factor = direction >= 0 ? 1 / ZOOM_FACTOR : ZOOM_FACTOR
  const newSpan = clamp(span * factor, minSpan, bounds.span)

  let newStart = clampedPivot - ratio * newSpan
  let newEnd = newStart + newSpan

  if (newStart < bounds.start) {
    newStart = bounds.start
    newEnd = bounds.start + newSpan
  }
  if (newEnd > bounds.end) {
    newEnd = bounds.end
    newStart = bounds.end - newSpan
  }

  return { viewStart: newStart, viewEnd: newEnd }
}

/**
 * Fits the viewport to a target range with padding and clamping.
 *
 * @param {{ rangeStart: number, rangeEnd: number, totalDuration: number, timelineMinimum?: number, widthPx?: number }} options
 * @returns {{ viewStart: number, viewEnd: number }} Fitted viewport.
 */
export function fitRangeToViewport({ rangeStart, rangeEnd, totalDuration, timelineMinimum = 0, widthPx = 0 }) {
  const bounds = getTimelineBounds(timelineMinimum, totalDuration)
  const start = clamp(rangeStart, bounds.start, bounds.end)
  const end = clamp(Math.max(start, rangeEnd), start, bounds.end)
  const rangeSpan = end - start
  const padding = rangeSpan * FIT_PADDING_RATIO

  let viewStart = start - padding
  let viewEnd = end + padding
  let span = viewEnd - viewStart

  const minimumSpan = Math.min(Math.max(FIT_MIN_SPAN, getMinimumZoomSpan(widthPx)), bounds.span)
  if (span < minimumSpan) {
    const halfExtra = (minimumSpan - span) / 2
    viewStart -= halfExtra
    viewEnd += halfExtra
    span = minimumSpan
  }

  span = Math.min(span, bounds.span)

  if (viewStart < bounds.start) {
    viewStart = bounds.start
    viewEnd = bounds.start + span
  }
  if (viewEnd > bounds.end) {
    viewEnd = bounds.end
    viewStart = bounds.end - span
  }

  return { viewStart, viewEnd }
}

/**
 * Shifts the viewport by a delta in seconds.
 *
 * @param {{ viewStart: number, viewEnd: number, deltaSeconds: number, totalDuration: number, timelineMinimum?: number }} options
 * @returns {{ viewStart: number, viewEnd: number }} Panned viewport.
 */
export function panViewport({ viewStart, viewEnd, deltaSeconds, totalDuration, timelineMinimum = 0 }) {
  const bounds = getTimelineBounds(timelineMinimum, totalDuration)
  if (bounds.span === 0) return { viewStart: bounds.start, viewEnd: bounds.end }
  const span = viewEnd - viewStart
  if (span >= bounds.span) return { viewStart, viewEnd }
  return clampToView(viewStart + deltaSeconds, viewEnd + deltaSeconds, bounds.end, bounds.start)
}

/**
 * Computes a viewport that keeps the playhead visible during playback.
 *
 * @param {{ playheadSecond: number, viewStart: number, viewEnd: number, totalDuration: number, timelineMinimum?: number }} options
 * @returns {{ viewStart: number, viewEnd: number }} Updated viewport.
 */
export function followPlayhead({ playheadSecond, viewStart, viewEnd, totalDuration, timelineMinimum = 0 }) {
  const bounds = getTimelineBounds(timelineMinimum, totalDuration)
  const span = viewEnd - viewStart
  if (span <= 0 || span >= bounds.span) return { viewStart, viewEnd }

  const followStart = viewStart + FOLLOW_EDGE_PADDING_RATIO * span
  const followEnd = viewEnd - FOLLOW_EDGE_PADDING_RATIO * span
  if (playheadSecond >= followStart && playheadSecond <= followEnd) return { viewStart, viewEnd }

  const playheadOffsetRatio = playheadSecond < followStart ? FOLLOW_EDGE_PADDING_RATIO : 1 - FOLLOW_EDGE_PADDING_RATIO
  const newStart = playheadSecond - playheadOffsetRatio * span
  return clampToView(newStart, newStart + span, bounds.end, bounds.start)
}

/**
 * Builds canonical fit targets for the current media shape.
 *
 * @param {{ totalDuration: number, widthPx?: number, hasVideo: boolean, videoSyncOffsetSeconds: number, importedVideoDuration: number, hasActivityData: boolean, activityDurationSeconds: number, fallbackDurationSeconds: number }} options
 * @returns {Array<{ id: 'all'|'video'|'activity', label: string, viewport: { viewStart: number, viewEnd: number } }>}
 */
export function buildFitTargets({
  totalDuration,
  widthPx = 0,
  hasVideo,
  videoSyncOffsetSeconds,
  importedVideoDuration,
  hasActivityData,
  activityDurationSeconds,
  fallbackDurationSeconds,
}) {
  const targets = []
  const timelineMinimum = getTimelineMinimum({ hasVideo, videoSyncOffsetSeconds })
  const componentRanges = []

  if (hasVideo) {
    const start = videoSyncOffsetSeconds
    const duration = importedVideoDuration
    const end = start + duration
    componentRanges.push({ start, end })
    targets.push({
      id: 'video',
      label: i18next.t('player.video', 'Video'),
      viewport: fitRangeToViewport({ rangeStart: start, rangeEnd: end, timelineMinimum, totalDuration, widthPx }),
    })
  }

  if (hasActivityData) {
    const duration = activityDurationSeconds > 0 ? activityDurationSeconds : fallbackDurationSeconds
    componentRanges.push({ start: 0, end: duration })
    targets.push({
      id: 'activity',
      label: i18next.t('player.activity', 'Activity'),
      viewport: fitRangeToViewport({ rangeStart: 0, rangeEnd: duration, timelineMinimum, totalDuration, widthPx }),
    })
  }

  const coversFullTimeline = componentRanges.some(({ start, end }) => start <= timelineMinimum && end >= totalDuration)
  if (!coversFullTimeline) {
    targets.unshift({ id: 'all', label: i18next.t('player.all', 'All'), viewport: fitToFull(totalDuration, timelineMinimum) })
  }

  return targets
}

/**
 * Resolves which canonical fit target matches the current viewport.
 *
 * @param {{ viewport: { viewStart: number, viewEnd: number }, targets: Array<{ id: string, viewport: { viewStart: number, viewEnd: number } }> }} options
 * @returns {string|null} Matching target id.
 */
export function getMatchingFitTargetId({ viewport, targets }) {
  return targets.find((target) => rangesMatch(viewport, target.viewport))?.id ?? null
}

function formatTickLabel(second, step) {
  if (step < 1) {
    return `${second.toFixed(1)}s`
  }
  return formatClockDuration(second)
}

/**
 * Computes major and minor ticks for the timeline axis.
 *
 * @param {{ viewStart: number, viewEnd: number, widthPx: number }} options
 * @returns {{ major: Array<{ second: number, x: number, label: string }>, minor: Array<{ second: number, x: number }> }}
 */
export function computeTimelineTicks({ viewStart, viewEnd, widthPx }) {
  const span = viewEnd - viewStart
  if (span <= 0 || widthPx <= 0) return { major: [], minor: [] }

  const pxPerSecond = widthPx / span
  const targetStep = TICK_TARGET_PX / pxPerSecond

  let step = NICE_STEPS[NICE_STEPS.length - 1]
  for (const candidate of NICE_STEPS) {
    if (candidate >= targetStep) {
      step = candidate
      break
    }
  }

  const minorStep = step / 5
  const firstMajor = Math.ceil(viewStart / step) * step
  const firstMinor = Math.ceil(viewStart / minorStep) * minorStep

  const major = []
  for (let second = firstMajor; second <= viewEnd; second += step) {
    const x = clamp(secondsToViewPx({ second, viewStart, viewEnd, widthPx }), 0, widthPx)
    major.push({ second, x, label: formatTickLabel(second, step) })
  }

  const minor = []
  for (let second = firstMinor; second <= viewEnd; second += minorStep) {
    const x = clamp(secondsToViewPx({ second, viewStart, viewEnd, widthPx }), 0, widthPx)
    minor.push({ second, x })
  }

  return { major, minor }
}
