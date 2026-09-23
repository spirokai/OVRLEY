import { VIDEO_SYNC_SIGNIFICANT_GAP_BASE_SECONDS, VIDEO_SYNC_SIGNIFICANT_GAP_CADENCE_MULTIPLIER } from '../data/videoSyncConstants'

/**
 * Calculates the median of an already-normalized numeric series.
 *
 * @param {number[]} values Values to summarize.
 * @returns {number} Median value, or zero when the series is empty.
 */
function median(values) {
  if (values.length === 0) return 0

  const ordered = [...values].sort((left, right) => left - right)
  const middle = Math.floor(ordered.length / 2)
  return ordered.length % 2 === 0 ? (ordered[middle - 1] + ordered[middle]) / 2 : ordered[middle]
}

/**
 * Aligns an optional parsed-activity series with the canonical elapsed-time array.
 *
 * @param {unknown[]|null|undefined} series Optional source series.
 * @param {number} length Required aligned length.
 * @returns {unknown[]} Aligned values with explicit nulls for missing entries.
 */
function projectAlignedSeries(series, length) {
  return Array.from({ length }, (_, index) => series?.[index] ?? null)
}

/**
 * Splits the activity where elapsed-time gaps exceed the cadence-aware threshold.
 *
 * @param {number[]} elapsedSeconds Canonical sample timestamps.
 * @returns {{startIndex: number, endIndex: number}[]} Contiguous sample segments.
 */
function buildSegments(elapsedSeconds) {
  if (elapsedSeconds.length === 0) return []

  const intervals = []
  for (let index = 1; index < elapsedSeconds.length; index += 1) {
    intervals.push(elapsedSeconds[index] - elapsedSeconds[index - 1])
  }

  const medianInterval = median(intervals)
  const significantGapSeconds = Math.max(VIDEO_SYNC_SIGNIFICANT_GAP_BASE_SECONDS, VIDEO_SYNC_SIGNIFICANT_GAP_CADENCE_MULTIPLIER * medianInterval)
  const segments = []
  let startIndex = 0

  for (let index = 1; index < elapsedSeconds.length; index += 1) {
    if (elapsedSeconds[index] - elapsedSeconds[index - 1] > significantGapSeconds) {
      segments.push({ startIndex, endIndex: index })
      startIndex = index
    }
  }

  segments.push({ startIndex, endIndex: elapsedSeconds.length })
  return segments
}

/**
 * Splits the canonical geographic course at missing coordinate pairs.
 * Single-point fragments are omitted because they cannot form a polyline.
 *
 * @param {object} parsedActivity Finalized canonical activity data.
 * @returns {{coordinate: number[], activitySecond: number}[][]} Contiguous timed course segments.
 */
export function buildActivityCourseSegments(parsedActivity) {
  if (parsedActivity === null) return []

  const segments = []
  let currentSegment = []

  parsedActivity.sample_course_points.forEach(([latitude, longitude], index) => {
    if (latitude === null || longitude === null) {
      if (currentSegment.length > 1) segments.push(currentSegment)
      currentSegment = []
    } else {
      currentSegment.push({ coordinate: [longitude, latitude], activitySecond: parsedActivity.sample_elapsed_seconds[index] })
    }
  })

  if (currentSegment.length > 1) segments.push(currentSegment)
  return segments
}

/**
 * Projects canonical parsed activity data into the one detector input shape.
 *
 * Missing speed, heading, and course values remain explicit nulls. The
 * detector receives aligned arrays and never needs to perform source-format
 * coercion or decide whether a metric is available.
 *
 * @param {object|null} parsedActivity Finalized canonical activity data.
 * @returns {{elapsedSeconds: number[], speed: (number|null)[], heading: (number|null)[], course: ([number|null, number|null]|null)[], availability: {speed: boolean, heading: boolean, course: boolean}, segments: {startIndex: number, endIndex: number}[]}}
 */
export function createActivitySyncInput(parsedActivity) {
  const elapsedSeconds = parsedActivity.sample_elapsed_seconds
  const speed = projectAlignedSeries(parsedActivity.speed, elapsedSeconds.length)
  const heading = projectAlignedSeries(parsedActivity.heading, elapsedSeconds.length)
  const course = projectAlignedSeries(parsedActivity.sample_course_points, elapsedSeconds.length)

  return {
    elapsedSeconds: [...elapsedSeconds],
    speed,
    heading,
    course,
    availability: {
      speed: speed.some((value) => value !== null),
      heading: heading.some((value) => value !== null),
      course: course.some((point) => point !== null && point[0] !== null && point[1] !== null),
    },
    segments: buildSegments(elapsedSeconds),
  }
}
