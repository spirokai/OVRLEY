import { VIDEO_SYNC_HEADING_SMOOTHING_WINDOW_SECONDS, VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS } from '../data/videoSyncConstants'

/**
 * Validates the user-owned turn threshold at the detector boundary.
 *
 * @param {number} turnThresholdDegrees Configured turn threshold in degrees.
 * @returns {void}
 * @throws {Error} When the threshold is not finite.
 */
function requireTurnThreshold(turnThresholdDegrees) {
  if (!Number.isFinite(turnThresholdDegrees)) {
    throw new Error('Manual video sync turn threshold must be finite')
  }
}

/**
 * Normalizes a heading to the canonical [0, 360) degree range.
 *
 * @param {number} degrees Heading in degrees.
 * @returns {number} Normalized heading.
 */
function normalizeHeading(degrees) {
  const normalized = degrees % 360
  return normalized < 0 ? normalized + 360 : normalized
}

/**
 * Calculates the shortest signed angular change between two headings.
 *
 * @param {number} current Current heading in degrees.
 * @param {number} previous Previous heading in degrees.
 * @returns {number} Signed change in the range (-180, 180] degrees.
 */
function signedHeadingDelta(current, previous) {
  let delta = current - previous
  while (delta > 180) delta -= 360
  while (delta <= -180) delta += 360
  return delta
}

/**
 * Finds contiguous heading runs within a timestamp segment.
 *
 * Missing headings terminate a run so smoothing and turning rates never bridge
 * unavailable data.
 *
 * @param {{heading: (number|null)[]}} input Detector input.
 * @param {{startIndex: number, endIndex: number}} segment Timestamp segment.
 * @returns {{startIndex: number, endIndex: number}[]} Contiguous heading runs.
 */
function findHeadingRuns(input, segment) {
  const runs = []
  let startIndex = null

  for (let index = segment.startIndex; index < segment.endIndex; index += 1) {
    if (input.heading[index] === null) {
      if (startIndex !== null) runs.push({ startIndex, endIndex: index })
      startIndex = null
    } else if (startIndex === null) {
      startIndex = index
    }
  }

  if (startIndex !== null) runs.push({ startIndex, endIndex: segment.endIndex })
  return runs
}

/**
 * Creates a monotonic integral reader for one heading run.
 *
 * The prefix sums contain elapsed-time-weighted unit heading vectors. The
 * reader interpolates the final partial interval for a requested timestamp.
 *
 * @param {{elapsedSeconds: number[], heading: (number|null)[]}} input Detector input.
 * @param {{startIndex: number, endIndex: number}} run Contiguous heading run.
 * @param {number[]} cosinePrefix Prefix sums of weighted cosine components.
 * @param {number[]} sinePrefix Prefix sums of weighted sine components.
 * @returns {(targetTime: number) => {cosine: number, sine: number}} Integral reader.
 */
function createHeadingIntegralReader(input, run, cosinePrefix, sinePrefix) {
  const times = input.elapsedSeconds
  const headings = input.heading
  const startTime = times[run.startIndex]
  const endTime = times[run.endIndex - 1]
  const lastIntervalIndex = run.endIndex - run.startIndex - 1
  let intervalIndex = 0

  return (targetTime) => {
    if (targetTime <= startTime) return { cosine: 0, sine: 0 }
    if (targetTime >= endTime) {
      return { cosine: cosinePrefix[lastIntervalIndex], sine: sinePrefix[lastIntervalIndex] }
    }

    while (intervalIndex < lastIntervalIndex && times[run.startIndex + intervalIndex + 1] <= targetTime) intervalIndex += 1

    const sampleIndex = run.startIndex + intervalIndex
    const radians = (normalizeHeading(headings[sampleIndex]) * Math.PI) / 180
    const elapsed = targetTime - times[sampleIndex]
    return {
      cosine: cosinePrefix[intervalIndex] + Math.cos(radians) * elapsed,
      sine: sinePrefix[intervalIndex] + Math.sin(radians) * elapsed,
    }
  }
}

/**
 * Smooths one heading run with an elapsed-time circular one-second window.
 *
 * @param {{elapsedSeconds: number[], heading: (number|null)[]}} input Detector input.
 * @param {{startIndex: number, endIndex: number}} run Contiguous heading run.
 * @returns {number[]} Smoothed headings aligned to the run's local indices.
 */
function smoothHeadingRun(input, run) {
  const localLength = run.endIndex - run.startIndex
  const times = input.elapsedSeconds
  const headings = input.heading
  const startTime = times[run.startIndex]
  const endTime = times[run.endIndex - 1]
  const halfWindow = VIDEO_SYNC_HEADING_SMOOTHING_WINDOW_SECONDS / 2
  const cosinePrefix = [0]
  const sinePrefix = [0]

  for (let localIndex = 0; localIndex < localLength - 1; localIndex += 1) {
    const sampleIndex = run.startIndex + localIndex
    const elapsed = times[sampleIndex + 1] - times[sampleIndex]
    const radians = (normalizeHeading(headings[sampleIndex]) * Math.PI) / 180
    cosinePrefix.push(cosinePrefix[localIndex] + Math.cos(radians) * elapsed)
    sinePrefix.push(sinePrefix[localIndex] + Math.sin(radians) * elapsed)
  }

  const integrateFromStart = createHeadingIntegralReader(input, run, cosinePrefix, sinePrefix)
  const integrateFromEnd = createHeadingIntegralReader(input, run, cosinePrefix, sinePrefix)

  return Array.from({ length: localLength }, (_, localIndex) => {
    const sampleTime = times[run.startIndex + localIndex]
    const windowStart = Math.max(startTime, sampleTime - halfWindow)
    const windowEnd = Math.min(endTime, sampleTime + halfWindow)
    const startIntegral = integrateFromStart(windowStart)
    const endIntegral = integrateFromEnd(windowEnd)
    const cosine = endIntegral.cosine - startIntegral.cosine
    const sine = endIntegral.sine - startIntegral.sine

    if (Math.hypot(cosine, sine) === 0) return normalizeHeading(headings[run.startIndex + localIndex])
    return normalizeHeading((Math.atan2(sine, cosine) * 180) / Math.PI)
  })
}

/**
 * Derives the one elapsed-time signed turning series used by both detection
 * and the future timeline graph. Values are degrees per second; missing or
 * first-in-run values are null and never bridge a gap.
 *
 * @param {object} input Detector input produced by createActivitySyncInput.
 * @returns {{time: number, value: number|null}[]} Timestamped signed turning rate.
 */
export function deriveTurningSeries(input) {
  const series = input.elapsedSeconds.map((time) => ({ time, value: null }))

  for (const segment of input.segments) {
    for (const run of findHeadingRuns(input, segment)) {
      const smoothedHeadings = smoothHeadingRun(input, run)
      for (let localIndex = 1; localIndex < smoothedHeadings.length; localIndex += 1) {
        const sampleIndex = run.startIndex + localIndex
        const previousIndex = sampleIndex - 1
        const elapsed = input.elapsedSeconds[sampleIndex] - input.elapsedSeconds[previousIndex]
        const change = signedHeadingDelta(smoothedHeadings[localIndex], smoothedHeadings[localIndex - 1])
        series[sampleIndex].value = change / elapsed
      }
    }
  }

  return series
}

/**
 * Creates a monotonic overlap checker for near-stop suppression intervals.
 *
 * @param {{start: number, end: number}[]} nearStopIntervals Sorted suppression intervals.
 * @returns {(startTime: number, endTime: number) => boolean} Overlap predicate.
 */
function createSuppressionChecker(nearStopIntervals) {
  let intervalIndex = 0

  return (startTime, endTime) => {
    while (intervalIndex < nearStopIntervals.length && nearStopIntervals[intervalIndex].end <= startTime) intervalIndex += 1
    const interval = nearStopIntervals[intervalIndex]
    return interval !== undefined && startTime < interval.end && endTime > interval.start
  }
}

/**
 * Creates a direction-coherent sliding accumulation.
 *
 * `intervals` and `windowChange` retain only the latest maximum-duration
 * window. Overlapping qualifying windows extend one event; once the window no
 * longer qualifies, same-direction drift cannot extend that event forever.
 *
 * @param {'rightTurn'|'leftTurn'} direction Turn direction.
 * @returns {{direction: 'rightTurn'|'leftTurn', intervals: {start: number, end: number, change: number}[], firstIntervalIndex: number, windowChange: number, totalChange: number, eventChangeAtEnd: number, event: object|null}} Turn accumulation state.
 */
function createTurnRun(direction) {
  return {
    direction,
    intervals: [],
    firstIntervalIndex: 0,
    windowChange: 0,
    totalChange: 0,
    eventChangeAtEnd: 0,
    event: null,
  }
}

/**
 * Trims an unqualified accumulation to the configured elapsed-time window.
 * The oldest interval may be retained fractionally so irregular sampling does
 * not move the effective duration boundary to a sample timestamp.
 *
 * @param {ReturnType<typeof createTurnRun>} run Directional accumulation.
 * @param {number} currentTime Current interval end.
 * @returns {void}
 */
function trimTurnWindow(run, currentTime) {
  const windowStart = currentTime - VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS
  while (run.firstIntervalIndex < run.intervals.length && run.intervals[run.firstIntervalIndex].end <= windowStart) {
    run.windowChange -= run.intervals[run.firstIntervalIndex].change
    run.firstIntervalIndex += 1
  }

  const first = run.intervals[run.firstIntervalIndex]
  if (first === undefined || first.start >= windowStart) return

  const retainedRatio = (first.end - windowStart) / (first.end - first.start)
  const retainedChange = first.change * retainedRatio
  run.windowChange += retainedChange - first.change
  run.intervals[run.firstIntervalIndex] = { start: windowStart, end: first.end, change: retainedChange }
}

/**
 * Locates the latest start that contributes exactly the qualifying angle.
 * Using the tight supporting interval keeps a long, low-amplitude drift before
 * a real turn out of the event geometry and makes its timing cadence-stable.
 *
 * @param {ReturnType<typeof createTurnRun>} run Qualified directional accumulation.
 * @param {number} turnThresholdDegrees Required accumulated magnitude.
 * @returns {number} Interpolated event start time.
 */
function findQualifyingStart(run, turnThresholdDegrees) {
  let remaining = turnThresholdDegrees
  for (let index = run.intervals.length - 1; index >= run.firstIntervalIndex; index -= 1) {
    const interval = run.intervals[index]
    const magnitude = Math.abs(interval.change)
    if (magnitude >= remaining) {
      return interval.end - (remaining / magnitude) * (interval.end - interval.start)
    }
    remaining -= magnitude
  }
  throw new Error('Qualified manual video sync turn is missing its supporting interval')
}

/**
 * Adds one interval to a coherent run and emits or extends its turn event.
 *
 * @param {ReturnType<typeof createTurnRun>} run Directional accumulation.
 * @param {{start: number, end: number, change: number}} interval Signed heading change interval.
 * @param {number} turnThresholdDegrees Required accumulated turn magnitude.
 * @param {object[]} events Output event list.
 * @returns {void}
 */
function addTurnInterval(run, interval, turnThresholdDegrees, events) {
  run.intervals.push(interval)
  run.windowChange += interval.change
  run.totalChange += interval.change
  trimTurnWindow(run, interval.end)
  if (Math.abs(run.windowChange) < turnThresholdDegrees) return

  const start = findQualifyingStart(run, turnThresholdDegrees)
  if (run.event !== null && start <= run.event.end) {
    run.event.end = interval.end
    run.event.signedChange += run.totalChange - run.eventChangeAtEnd
    run.event.representativeTime = (run.event.start + run.event.end) / 2
    run.eventChangeAtEnd = run.totalChange
    return
  }

  const signedThreshold = run.direction === 'rightTurn' ? turnThresholdDegrees : -turnThresholdDegrees
  const event = {
    type: run.direction,
    start,
    end: interval.end,
    signedChange: signedThreshold,
    representativeTime: (start + interval.end) / 2,
  }
  run.event = event
  run.eventChangeAtEnd = run.totalChange
  events.push(event)
}

/**
 * Detects turns from a previously derived signed turning-rate series.
 *
 * @param {{elapsedSeconds: number[], segments: {startIndex: number, endIndex: number}[]}} input Detector input.
 * @param {{time: number, value: number|null}[]} turningSeries Signed turning-rate series.
 * @param {number} turnThresholdDegrees Required accumulated turn magnitude.
 * @param {{start: number, end: number}[]} nearStopIntervals Intervals that suppress turn accumulation.
 * @returns {object[]} Detected directional turn events.
 */
function detectTurnsFromSeries(input, turningSeries, turnThresholdDegrees, nearStopIntervals) {
  const events = []
  const isSuppressed = createSuppressionChecker(nearStopIntervals)

  for (const segment of input.segments) {
    let run = null

    for (let sampleIndex = segment.startIndex + 1; sampleIndex < segment.endIndex; sampleIndex += 1) {
      const rate = turningSeries[sampleIndex].value
      const previousTime = input.elapsedSeconds[sampleIndex - 1]
      const currentTime = input.elapsedSeconds[sampleIndex]

      if (rate === null || isSuppressed(previousTime, currentTime)) {
        run = null
        continue
      }

      const delta = rate * (currentTime - previousTime)
      if (delta === 0) continue

      const direction = delta > 0 ? 'rightTurn' : 'leftTurn'
      if (run === null || run.direction !== direction) run = createTurnRun(direction)
      addTurnInterval(run, { start: previousTime, end: currentTime, change: delta }, turnThresholdDegrees, events)
    }
  }

  return events.map((event, index) => ({ ...event, id: `${event.type}-${index}` }))
}

/**
 * Detects directional turns from the shared signed turning series.
 *
 * @param {object} input Detector input produced by createActivitySyncInput.
 * @param {{turnThresholdDegrees: number, nearStopIntervals?: {start: number, end: number}[]}} settings Direction threshold and near-stop intervals.
 * @returns {object[]} Detected directional turn events.
 */
export function detectTurns(input, { turnThresholdDegrees, nearStopIntervals = [] }) {
  requireTurnThreshold(turnThresholdDegrees)
  return detectTurnsFromSeries(input, deriveTurningSeries(input), turnThresholdDegrees, nearStopIntervals)
}

/**
 * Detects turns while reusing a caller-provided derived series.
 *
 * @param {object} input Detector input produced by createActivitySyncInput.
 * @param {{time: number, value: number|null}[]} turningSeries Shared turning series.
 * @param {{turnThresholdDegrees: number, nearStopIntervals?: {start: number, end: number}[]}} settings Detection settings.
 * @returns {object[]} Detected directional turn events.
 */
export function detectTurnsFromDerivedSeries(input, turningSeries, { turnThresholdDegrees, nearStopIntervals = [] }) {
  requireTurnThreshold(turnThresholdDegrees)
  return detectTurnsFromSeries(input, turningSeries, turnThresholdDegrees, nearStopIntervals)
}
