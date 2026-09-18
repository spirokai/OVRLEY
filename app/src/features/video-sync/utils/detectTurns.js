import {
  VIDEO_SYNC_TURN_BOUNDARY_RATE_DEGREES_PER_SECOND,
  VIDEO_SYNC_TURN_ENTRY_RATE_DEGREES_PER_SECOND,
  VIDEO_SYNC_TURN_EXIT_DWELL_SECONDS,
  VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS,
  VIDEO_SYNC_TURN_REVERSAL_TOLERANCE_DEGREES,
  VIDEO_SYNC_TURN_STATIONARY_SPEED_METERS_PER_SECOND,
  VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES,
} from '../data/videoSyncConstants'

/** Validates the user-owned angle threshold once at the detector boundary. */
function requireTurnThreshold(turnThresholdDegrees) {
  const { min, max } = VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES
  if (!Number.isFinite(turnThresholdDegrees) || turnThresholdDegrees < min || turnThresholdDegrees > max) {
    throw new Error(`Manual video sync turn threshold must be between ${min} and ${max} degrees`)
  }
}

/** Returns the shortest signed heading change in (-180, 180] degrees. */
function signedHeadingDelta(current, previous) {
  const delta = (((current - previous + 180) % 360) + 360) % 360
  return delta === 0 ? 180 : delta - 180
}

/**
 * Derives interval-average turn rates from finalized, already-smoothed heading.
 * Each value describes the interval ending at its timestamp. Missing headings
 * and timestamp segments break the series; no additional smoothing shifts it.
 *
 * @param {object} input Canonical input from createActivitySyncInput.
 * @returns {{time: number, value: number|null}[]} Signed rates in degrees/second.
 */
export function deriveTurningSeries(input) {
  const series = input.elapsedSeconds.map((time) => ({ time, value: null }))
  for (const segment of input.segments) {
    for (let index = segment.startIndex + 1; index < segment.endIndex; index += 1) {
      const previous = input.heading[index - 1]
      const current = input.heading[index]
      if (previous === null || current === null) continue
      const elapsed = input.elapsedSeconds[index] - input.elapsedSeconds[index - 1]
      series[index].value = signedHeadingDelta(current, previous) / elapsed
    }
  }
  return series
}

/**
 * Suppresses heading only near standstill, independently of stop-landmark tuning.
 * Speed crossings are linearly interpolated. Missing speed cannot establish
 * standstill, and neither missing samples nor timestamp gaps are bridged.
 */
function deriveNearStopIntervals(input) {
  const intervals = []
  const threshold = VIDEO_SYNC_TURN_STATIONARY_SPEED_METERS_PER_SECOND
  for (const segment of input.segments) {
    for (let index = segment.startIndex + 1; index < segment.endIndex; index += 1) {
      const previousSpeed = input.speed[index - 1]
      const speed = input.speed[index]
      if (previousSpeed === null || speed === null || (previousSpeed > threshold && speed > threshold)) continue

      const previousTime = input.elapsedSeconds[index - 1]
      const time = input.elapsedSeconds[index]
      let start = previousTime
      let end = time
      if (previousSpeed > threshold || speed > threshold) {
        const crossing = previousTime + ((threshold - previousSpeed) / (speed - previousSpeed)) * (time - previousTime)
        if (previousSpeed > threshold) start = crossing
        else end = crossing
      }
      if (start === end) continue
      const previousInterval = intervals[intervals.length - 1]
      if (previousInterval !== undefined && previousInterval.end === start) previousInterval.end = end
      else intervals.push({ start, end })
    }
  }
  return intervals
}

/**
 * Splits sample intervals at sorted suppression boundaries. Null-rate pieces
 * close episodes; usable fractions retain their original interval-average rate.
 */
function createIntervalReader(nearStopIntervals) {
  let index = 0
  return function* readIntervals(start, end, rate) {
    while (index < nearStopIntervals.length && nearStopIntervals[index].end <= start) index += 1
    while (index < nearStopIntervals.length && nearStopIntervals[index].start < end) {
      const suppressed = nearStopIntervals[index]
      if (start < suppressed.start) yield { start, end: suppressed.start, rate }
      const suppressedEnd = Math.min(end, suppressed.end)
      yield { start: Math.max(start, suppressed.start), end: suppressedEnd, rate: null }
      start = suppressedEnd
      if (suppressed.end > end) break
      index += 1
    }
    if (start < end) yield { start, end, rate }
  }
}

/**
 * Qualifies one complete directional episode. Boundaries use the lower rate;
 * qualification requires the higher rate, actual net angle, and bounded duration.
 * Short internal quiet periods and minor corrections contribute their signed angle.
 */
function emitTurn(intervals, first, end, direction, turnThresholdDegrees, events) {
  const boundaryRate = VIDEO_SYNC_TURN_BOUNDARY_RATE_DEGREES_PER_SECOND
  while (first < end && Math.abs(intervals[first].rate) < boundaryRate) first += 1
  while (end > first && Math.abs(intervals[end - 1].rate) < boundaryRate) end -= 1
  if (first === end) return

  let signedChange = 0
  let peakRate = 0
  for (let index = first; index < end; index += 1) {
    const interval = intervals[index]
    signedChange += interval.rate * (interval.end - interval.start)
    peakRate = Math.max(peakRate, direction * interval.rate)
  }
  const startTime = intervals[first].start
  const endTime = intervals[end - 1].end
  if (
    peakRate < VIDEO_SYNC_TURN_ENTRY_RATE_DEGREES_PER_SECOND ||
    direction * signedChange < turnThresholdDegrees ||
    endTime - startTime > VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS
  ) {
    return
  }

  const type = direction > 0 ? 'rightTurn' : 'leftTurn'
  events.push({
    id: `${type}-${events.length}`,
    type,
    start: startTime,
    end: endTime,
    signedChange,
    representativeTime: (startTime + endTime) / 2,
  })
}

/**
 * Splits an episode at confirmed heading reversals. The running heading extremum
 * anchors a pending reversal, so small same-direction samples do not erase it.
 * On confirmation the new candidate includes every interval after that extremum.
 */
function finishEpisode(intervals, turnThresholdDegrees, events) {
  if (intervals.length === 0) return
  let first = 0
  let direction = Math.sign(intervals[0].rate)
  let headingChange = 0
  let extremeChange = 0
  let extremeEnd = 0

  for (let index = 0; index < intervals.length; index += 1) {
    const interval = intervals[index]
    headingChange += interval.rate * (interval.end - interval.start)
    if (direction * (headingChange - extremeChange) >= 0) {
      extremeChange = headingChange
      extremeEnd = index + 1
    } else if (direction * (extremeChange - headingChange) > VIDEO_SYNC_TURN_REVERSAL_TOLERANCE_DEGREES) {
      emitTurn(intervals, first, extremeEnd, direction, turnThresholdDegrees, events)
      first = extremeEnd
      direction = -direction
      extremeChange = headingChange
      extremeEnd = index + 1
    }
  }
  emitTurn(intervals, first, intervals.length, direction, turnThresholdDegrees, events)
}

/**
 * Collects episodes with elapsed-time exit dwell. Quiet intervals are provisional:
 * a short dip is included if turning resumes, while sustained quiet closes at
 * the last active boundary, without including the dwell or an EMA's low-rate tail.
 */
function createEpisodeCollector(turnThresholdDegrees, events) {
  let intervals = []
  let quietIntervals = []

  function finish() {
    finishEpisode(intervals, turnThresholdDegrees, events)
    intervals = []
    quietIntervals = []
  }

  function add(interval) {
    if (interval.rate === null) {
      finish()
      return
    }
    if (Math.abs(interval.rate) < VIDEO_SYNC_TURN_BOUNDARY_RATE_DEGREES_PER_SECOND) {
      if (intervals.length === 0) return
      quietIntervals.push(interval)
      if (interval.end - quietIntervals[0].start >= VIDEO_SYNC_TURN_EXIT_DWELL_SECONDS) finish()
      return
    }
    for (const quiet of quietIntervals) intervals.push(quiet)
    quietIntervals = []
    intervals.push(interval)
  }

  return { add, finish }
}

/** Processes contiguous sample intervals without connecting across missing data. */
function detectTurnsFromSeries(input, turningSeries, turnThresholdDegrees, nearStopIntervals) {
  const events = []
  const collector = createEpisodeCollector(turnThresholdDegrees, events)
  const readIntervals = createIntervalReader(nearStopIntervals)
  for (const segment of input.segments) {
    for (let index = segment.startIndex + 1; index < segment.endIndex; index += 1) {
      const start = input.elapsedSeconds[index - 1]
      const end = input.elapsedSeconds[index]
      for (const interval of readIntervals(start, end, turningSeries[index].value)) collector.add(interval)
    }
    collector.finish()
  }
  return events
}

/**
 * Detects turns from finalized heading using rate hysteresis and net turn angle.
 * Optional suppression intervals override the default standstill speed gate.
 *
 * @param {object} input Canonical input from createActivitySyncInput.
 * @param {{turnThresholdDegrees: number, nearStopIntervals?: {start: number, end: number}[]}} settings Detection settings.
 * @returns {object[]} Directional events with physical interval boundaries.
 */
export function detectTurns(input, settings) {
  return detectTurnsFromDerivedSeries(input, deriveTurningSeries(input), settings)
}

/**
 * Detects turns using the same interval-average rates displayed by the graph.
 * Optional suppression intervals must be sorted and non-overlapping; absent
 * intervals are derived from the canonical speed channel at the standstill gate.
 *
 * @param {object} input Canonical input from createActivitySyncInput.
 * @param {{time: number, value: number|null}[]} turningSeries Shared turning series.
 * @param {{turnThresholdDegrees: number, nearStopIntervals?: {start: number, end: number}[]}} settings Detection settings.
 * @returns {object[]} Directional events with net signed angle and midpoint time.
 */
export function detectTurnsFromDerivedSeries(input, turningSeries, { turnThresholdDegrees, nearStopIntervals = deriveNearStopIntervals(input) }) {
  requireTurnThreshold(turnThresholdDegrees)
  return detectTurnsFromSeries(input, turningSeries, turnThresholdDegrees, nearStopIntervals)
}
