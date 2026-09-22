import { detectStops } from './detectStops'
import { deriveTurningSeries, detectTurnsFromDerivedSeries } from './detectTurns'

/** @returns {object} Empty canonical detected-event model. */
export function createEmptyVideoSyncDetection() {
  return {
    availability: { speed: false, heading: false, course: false },
    stops: [],
    turns: [],
    location: null,
    graphSeries: { speed: [], turning: [] },
  }
}

/**
 * Detects events from the canonical feature ingress projection.
 *
 * Callers that also need availability or graph input can project the activity
 * once and reuse that immutable input for eligibility and calculation.
 *
 * @param {object} input Detector input produced by createActivitySyncInput.
 * @param {{speedThresholdKmh: number, turnThresholdDegrees: number}} settings Physical detector thresholds.
 * @param {object|null} location Existing user-selected location event to preserve across automatic detection.
 * @returns {{availability: {speed: boolean, heading: boolean, course: boolean}, stops: object[], turns: object[], location: object|null, graphSeries: {speed: {time: number, value: number|null}[], turning: {time: number, value: number|null}[]}}} Detector result.
 */
export function detectActivityEventsFromInput(input, settings, location) {
  const stops = detectStops(input, settings)
  const turning = deriveTurningSeries(input)
  const turns = detectTurnsFromDerivedSeries(input, turning, {
    turnThresholdDegrees: settings.turnThresholdDegrees,
  })

  return {
    availability: input.availability,
    stops,
    turns,
    location,
    graphSeries: {
      speed: input.elapsedSeconds.map((time, index) => ({ time, value: input.speed[index] })),
      turning,
    },
  }
}
