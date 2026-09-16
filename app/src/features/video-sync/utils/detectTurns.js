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
 * Creates an unqualified directional turn accumulation.
 *
 * @param {'rightTurn'|'leftTurn'} direction Turn direction.
 * @param {number} startTime Accumulation start timestamp.
 * @returns {{direction: 'rightTurn'|'leftTurn', startTime: number, signedChange: number, event: object|null}} Turn accumulation state.
 */
function createTurnRun(direction, startTime) {
  return {
    direction,
    startTime,
    signedChange: 0,
    event: null,
  }
}

/**
 * Closes a qualified turn accumulation at a boundary.
 *
 * @param {{startTime: number, signedChange: number, event: object|null}|null} run Turn accumulation state.
 * @param {number} endTime Boundary timestamp.
 * @returns {void}
 */
function finalizeTurnRun(run, endTime) {
  if (run === null || run.event === null) return
  run.event.end = Math.max(run.startTime, endTime)
  run.event.signedChange = run.signedChange
  run.event.representativeTime = (run.event.start + run.event.end) / 2
}

/**
 * Determines whether the next rate sample needs a new accumulation.
 *
 * @param {{direction: string, startTime: number}|null} run Current accumulation.
 * @param {'rightTurn'|'leftTurn'} direction Current sample direction.
 * @param {number} currentTime Current sample timestamp.
 * @returns {boolean} Whether a new accumulation must start.
 */
function shouldStartNewTurnRun(run, direction, currentTime) {
  return run === null || run.direction !== direction || currentTime - run.startTime > VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS
}

/**
 * Adds one signed angular delta and creates an event when the threshold crosses.
 *
 * @param {{direction: 'rightTurn'|'leftTurn', startTime: number, signedChange: number, event: object|null}} run Turn accumulation state.
 * @param {number} delta Signed angular delta for the current interval.
 * @param {number} previousTime Previous sample timestamp.
 * @param {number} currentTime Current sample timestamp.
 * @param {number} turnThresholdDegrees Required accumulated magnitude.
 * @param {object[]} events Output event list.
 * @returns {void}
 */
function qualifyTurnRun(run, delta, previousTime, currentTime, turnThresholdDegrees, events) {
  const previousMagnitude = Math.abs(run.signedChange)
  run.signedChange += delta

  if (run.event !== null || Math.abs(run.signedChange) < turnThresholdDegrees) return

  const remainingAngle = turnThresholdDegrees - previousMagnitude
  const crossingRatio = Math.max(0, Math.min(1, remainingAngle / Math.abs(delta)))
  const crossingTime = previousTime + crossingRatio * (currentTime - previousTime)
  if (crossingTime - run.startTime > VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS) return

  run.event = {
    type: run.direction,
    start: run.startTime,
    end: currentTime,
    signedChange: run.signedChange,
    representativeTime: (run.startTime + currentTime) / 2,
  }
  events.push(run.event)
}

/**
 * Extends an already-qualified event through the current sample.
 *
 * @param {{startTime: number, signedChange: number, event: object|null}|null} run Turn accumulation state.
 * @param {number} currentTime Current sample timestamp.
 * @returns {void}
 */
function extendQualifiedTurnRun(run, currentTime) {
  if (run === null || run.event === null) return
  run.event.end = currentTime
  run.event.signedChange = run.signedChange
  run.event.representativeTime = (run.event.start + run.event.end) / 2
}

/**
 * Merges adjacent or overlapping events that share a direction.
 *
 * @param {object[]} events Chronologically emitted turn events.
 * @returns {object[]} Merged events with stable directional identifiers.
 */
function mergeTurnEvents(events) {
  const merged = []

  for (const event of events) {
    const previous = merged.at(-1)
    if (previous && previous.type === event.type && event.start <= previous.end) {
      previous.end = Math.max(previous.end, event.end)
      previous.signedChange += event.signedChange
      previous.representativeTime = (previous.start + previous.end) / 2
      continue
    }
    merged.push({ ...event })
  }

  return merged.map((event, index) => ({ ...event, id: `${event.type}-${index}` }))
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
        finalizeTurnRun(run, previousTime)
        run = null
        continue
      }

      const delta = rate * (currentTime - previousTime)
      if (delta === 0) {
        extendQualifiedTurnRun(run, currentTime)
        continue
      }

      const direction = delta > 0 ? 'rightTurn' : 'leftTurn'
      if (shouldStartNewTurnRun(run, direction, currentTime)) {
        finalizeTurnRun(run, previousTime)
        run = createTurnRun(direction, previousTime)
      }

      qualifyTurnRun(run, delta, previousTime, currentTime, turnThresholdDegrees, events)
      extendQualifiedTurnRun(run, currentTime)
    }

    finalizeTurnRun(run, input.elapsedSeconds[segment.endIndex - 1])
  }

  return mergeTurnEvents(events)
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
