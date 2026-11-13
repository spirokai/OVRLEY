/**
 * Pure timeline geometry helpers for pointer, clip, and playhead math.
 */

import { clamp } from '@/lib/utils'

/** Snap distance on screen, in CSS pixels. */
export const SNAP_THRESHOLD_PX = 10

/**
 * Rounds a pixel value to the active device pixel grid.
 *
 * @param {number} value Pixel value.
 * @param {number} pixelRatio Device pixel ratio.
 * @returns {number} Rounded pixel value.
 */
export function roundToDevicePixel(value, pixelRatio = 1) {
  const safeRatio = Number(pixelRatio) || 1
  return Math.round((Number(value) || 0) * safeRatio) / safeRatio
}

/**
 * Snaps a container-relative pixel value onto whole screen pixels, accounting for the container's own screen offset.
 *
 * @param {number} value Container-relative CSS pixel value.
 * @param {{ ratio: number, origin: number }} pixelGrid Screen pixels per CSS pixel and the container's screen x in screen pixels.
 * @returns {number} Container-relative CSS pixel value that lands on a screen pixel edge.
 */
export function snapToScreenPixel(value, { ratio, origin }) {
  return (Math.round(origin + value * ratio) - origin) / ratio
}

/**
 * Converts a pointer's clientX to a clamped timeline second.
 *
 * @param {{ clientX: number, rect: DOMRect|{ left?: number, width?: number }, viewStart: number, viewEnd: number, widthPx: number, timelineMinimum?: number, totalDuration: number }} options
 * @returns {number} Clamped timeline second.
 */
export function pointerToSecond({ clientX, rect, viewStart, viewEnd, widthPx, timelineMinimum = 0, totalDuration }) {
  const span = viewEnd - viewStart
  const effectiveWidth = rect?.width || widthPx
  if (span <= 0 || effectiveWidth <= 0) return clamp(timelineMinimum, timelineMinimum, totalDuration)
  const ratio = (clientX - (rect?.left || 0)) / effectiveWidth
  const second = viewStart + ratio * span
  return clamp(second, timelineMinimum, totalDuration)
}

/**
 * Converts a timeline second to a pixel position within the current viewport.
 *
 * @param {{ second: number, viewStart: number, viewEnd: number, widthPx: number }} options
 * @returns {number} Pixel position from the left edge.
 */
export function secondsToViewPx({ second, viewStart, viewEnd, widthPx }) {
  const span = viewEnd - viewStart
  if (span <= 0 || widthPx <= 0) return 0
  return ((second - viewStart) / span) * widthPx
}

/**
 * Converts a horizontal pixel delta to a second delta in the current viewport.
 *
 * @param {{ deltaPx: number, viewStart: number, viewEnd: number, widthPx: number }} options
 * @returns {number} Second delta.
 */
export function viewPxToSeconds({ deltaPx, viewStart, viewEnd, widthPx }) {
  const span = viewEnd - viewStart
  if (span <= 0 || widthPx <= 0) return 0
  return (deltaPx / widthPx) * span
}

/**
 * Computes the pixel geometry of a clip in the current viewport.
 *
 * @param {{ startSecond: number, durationSeconds: number, viewStart: number, viewEnd: number, widthPx: number }} options
 * @returns {{ x: number, width: number, isVisible: boolean }} Pixel position, width, and visibility.
 */
export function getClipGeometry({ startSecond, durationSeconds, viewStart, viewEnd, widthPx }) {
  const span = viewEnd - viewStart
  const clipEnd = startSecond + durationSeconds
  if (span <= 0 || widthPx <= 0 || clipEnd <= viewStart || startSecond >= viewEnd || durationSeconds <= 0) {
    return { x: 0, width: 0, isVisible: false }
  }
  const x = ((startSecond - viewStart) / span) * widthPx
  const width = (durationSeconds / span) * widthPx
  return { x, width, isVisible: true }
}

/**
 * Snaps a proposed shift of a dragged clip or cut so that one moving edge lands on a fixed edge.
 *
 * The threshold is expressed in screen pixels and converted to timeline seconds for
 * the current viewport.
 *
 * @param {{ proposedDelta: number, movingEdges: number[], fixedEdges: number[], viewStart: number, viewEnd: number, widthPx: number, thresholdPx?: number }} options Snap inputs.
 * @returns {{ delta: number, guidelineSecond: number|null }} Snapped shift and active guideline.
 */
export function snapClipDelta({ proposedDelta, movingEdges, fixedEdges, viewStart, viewEnd, widthPx, thresholdPx = SNAP_THRESHOLD_PX }) {
  const span = viewEnd - viewStart
  const thresholdSeconds = span > 0 && widthPx > 0 ? (thresholdPx / widthPx) * span : 0

  let bestDistance = Infinity
  let delta = proposedDelta
  let guidelineSecond = null

  for (const movingEdge of movingEdges) {
    for (const fixedEdge of fixedEdges) {
      const candidate = fixedEdge - movingEdge
      const distance = Math.abs(proposedDelta - candidate)
      if (distance <= thresholdSeconds && distance < bestDistance) {
        bestDistance = distance
        delta = candidate
        guidelineSecond = fixedEdge
      }
    }
  }

  return { delta, guidelineSecond }
}

/** Timeline lane id of the external activity. */
export const ACTIVITY_LANE_ID = 'activity'

/**
 * Returns the timeline lane id of an imported video's clip.
 *
 * @param {string} videoId Video id.
 * @returns {string} Lane id `video:<videoId>`.
 */
export function videoLaneId(videoId) {
  return `video:${videoId}`
}

/**
 * Returns the imported video addressed by a timeline lane, or null for the activity lane.
 *
 * @param {string} laneId Timeline lane id: `activity` or `video:<videoId>`.
 * @returns {string|null} Video id.
 */
export function videoIdFromLaneId(laneId) {
  return laneId === ACTIVITY_LANE_ID ? null : laneId.slice(videoLaneId('').length)
}
