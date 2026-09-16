import { createActivitySyncInput } from './activitySyncInput'
import { detectStopState } from './detectStops'
import { deriveTurningSeries, detectTurnsFromDerivedSeries } from './detectTurns'

/**
 * Detects all manual video-sync activity events from one canonical activity
 * projection. The turning series is derived once and returned for graph use.
 *
 * @param {object|null} parsedActivity Finalized canonical activity data.
 * @param {{speedThresholdKmh: number, turnThresholdDegrees: number}} settings Physical detector thresholds.
 * @returns {{availability: {speed: boolean, heading: boolean, course: boolean}, stops: object[], turns: object[], graphSeries: {speed: {time: number, value: number|null}[], turning: {time: number, value: number|null}[]}}} Detector result.
 */
export function detectActivityEvents(parsedActivity, settings) {
  const input = createActivitySyncInput(parsedActivity)
  const stopState = detectStopState(input, settings)
  const turning = deriveTurningSeries(input)
  const turns = detectTurnsFromDerivedSeries(input, turning, {
    turnThresholdDegrees: settings.turnThresholdDegrees,
    nearStopIntervals: stopState.nearStopIntervals,
  })

  return {
    availability: input.availability,
    stops: stopState.stops,
    turns,
    graphSeries: {
      speed: input.elapsedSeconds.map((time, index) => ({ time, value: input.speed[index] })),
      turning,
    },
  }
}
