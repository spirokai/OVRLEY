import {
  VIDEO_SYNC_KMH_TO_METERS_PER_SECOND,
  VIDEO_SYNC_STOP_ENTRY_DWELL_SECONDS,
  VIDEO_SYNC_STOP_EXIT_DWELL_SECONDS,
  VIDEO_SYNC_STOP_EXIT_HYSTERESIS_KMH,
  VIDEO_SYNC_STOP_MAXIMUM_DURATION_SECONDS,
} from '../data/videoSyncConstants'

function interpolateCrossingTime(previousTime, previousValue, currentTime, currentValue, threshold) {
  const valueDelta = currentValue - previousValue
  if (valueDelta === 0) return previousTime

  const ratio = (threshold - previousValue) / valueDelta
  return previousTime + ratio * (currentTime - previousTime)
}

const STOP_PHASES = Object.freeze({
  UNESTABLISHED: 'unestablished',
  ESTABLISHING_MOVEMENT: 'establishingMovement',
  MOVING: 'moving',
  ENTERING_STOP: 'enteringStop',
  STOPPED: 'stopped',
  EXITING_STOP: 'exitingStop',
})

function createState(phase, values = {}) {
  return {
    phase,
    movementStartedAt: values.movementStartedAt ?? null,
    nearStopStartedAt: values.nearStopStartedAt ?? null,
    stopCandidateStartedAt: values.stopCandidateStartedAt ?? null,
    exitStartedAt: values.exitStartedAt ?? null,
    stopEvent: values.stopEvent ?? null,
    stopIntervalOpen: values.stopIntervalOpen ?? false,
  }
}

function extendStopInterval(state, endTime) {
  if (state.stopEvent === null || !state.stopIntervalOpen) return
  const interval = state.stopEvent.lowSpeedInterval
  interval.end = Math.min(endTime, interval.start + VIDEO_SYNC_STOP_MAXIMUM_DURATION_SECONDS)
  if (interval.end === interval.start + VIDEO_SYNC_STOP_MAXIMUM_DURATION_SECONDS) state.stopIntervalOpen = false
}

function enterStopped(previousState, nearStopStartedAt, stopCandidateStartedAt = null) {
  return createState(STOP_PHASES.STOPPED, {
    nearStopStartedAt,
    stopCandidateStartedAt,
    stopEvent: previousState.stopEvent,
    stopIntervalOpen: previousState.stopIntervalOpen,
  })
}

function establishStopEvent(state, time, stops) {
  const stop = {
    id: `stop-${stops.length}`,
    type: 'stop',
    time: state.stopCandidateStartedAt,
    lowSpeedInterval: { start: state.nearStopStartedAt, end: time },
  }
  stops.push(stop)
  return createState(STOP_PHASES.STOPPED, {
    nearStopStartedAt: state.nearStopStartedAt,
    stopEvent: stop,
    stopIntervalOpen: true,
  })
}

function stateForFirstSample(speed, time, threshold, movementThreshold) {
  if (speed <= threshold) return enterStopped(createState(STOP_PHASES.UNESTABLISHED), time)
  if (speed > movementThreshold) return createState(STOP_PHASES.ESTABLISHING_MOVEMENT, { movementStartedAt: time })
  return createState(STOP_PHASES.UNESTABLISHED)
}

// A sample may finish one phase and enter the next in the same transition.
function advanceState(state, { previousTime, previousSpeed, time, speed, threshold, movementThreshold }, stops) {
  let nextState = state

  while (true) {
    switch (nextState.phase) {
      case STOP_PHASES.UNESTABLISHED:
        if (speed <= threshold) {
          nextState = enterStopped(nextState, time)
          continue
        }
        if (speed > movementThreshold) return createState(STOP_PHASES.ESTABLISHING_MOVEMENT, { movementStartedAt: time })
        return nextState

      case STOP_PHASES.ESTABLISHING_MOVEMENT: {
        const movementEndTime =
          speed > movementThreshold
            ? time
            : previousSpeed > movementThreshold
              ? interpolateCrossingTime(previousTime, previousSpeed, time, speed, movementThreshold)
              : previousTime
        const movementEstablished = movementEndTime - nextState.movementStartedAt >= VIDEO_SYNC_STOP_ENTRY_DWELL_SECONDS

        if (!movementEstablished) {
          return speed > movementThreshold ? nextState : createState(STOP_PHASES.UNESTABLISHED)
        }

        nextState = createState(STOP_PHASES.MOVING, { movementStartedAt: nextState.movementStartedAt })
        continue
      }

      case STOP_PHASES.MOVING:
        if (previousSpeed > threshold && speed <= threshold) {
          const crossingTime = interpolateCrossingTime(previousTime, previousSpeed, time, speed, threshold)
          nextState = createState(STOP_PHASES.ENTERING_STOP, {
            nearStopStartedAt: crossingTime,
            stopCandidateStartedAt: crossingTime,
          })
          continue
        }
        return nextState

      case STOP_PHASES.ENTERING_STOP:
        if (speed <= threshold && time - nextState.stopCandidateStartedAt >= VIDEO_SYNC_STOP_ENTRY_DWELL_SECONDS) {
          return establishStopEvent(nextState, time, stops)
        }
        if (speed > threshold) {
          nextState = enterStopped(nextState, nextState.nearStopStartedAt)
          continue
        }
        return nextState

      case STOP_PHASES.STOPPED:
        if (nextState.stopIntervalOpen && previousSpeed <= threshold && speed > threshold) {
          extendStopInterval(nextState, interpolateCrossingTime(previousTime, previousSpeed, time, speed, threshold))
          nextState.stopIntervalOpen = false
        } else if (speed <= threshold) {
          extendStopInterval(nextState, time)
        }
        if (speed > movementThreshold) {
          const exitStartedAt =
            previousSpeed <= movementThreshold ? interpolateCrossingTime(previousTime, previousSpeed, time, speed, movementThreshold) : previousTime
          return createState(STOP_PHASES.EXITING_STOP, {
            nearStopStartedAt: nextState.nearStopStartedAt,
            exitStartedAt,
            stopEvent: nextState.stopEvent,
            stopIntervalOpen: nextState.stopIntervalOpen,
          })
        }
        return nextState

      case STOP_PHASES.EXITING_STOP:
        if (speed > movementThreshold && time - nextState.exitStartedAt >= VIDEO_SYNC_STOP_EXIT_DWELL_SECONDS) {
          const exitTime = nextState.exitStartedAt
          extendStopInterval(nextState, exitTime)
          return createState(STOP_PHASES.MOVING, { movementStartedAt: exitTime })
        }
        if (speed <= movementThreshold) {
          return createState(STOP_PHASES.STOPPED, {
            nearStopStartedAt: nextState.nearStopStartedAt,
            stopEvent: nextState.stopEvent,
            stopIntervalOpen: nextState.stopIntervalOpen,
          })
        }
        return nextState

      default:
        throw new Error(`Unknown manual video sync stop phase: ${nextState.phase}`)
    }
  }
}

/**
 * Detects stop transitions from established movement.
 *
 * @param {{elapsedSeconds: number[], speed: (number|null)[], segments: {startIndex: number, endIndex: number}[]}} input Detector input produced by createActivitySyncInput.
 * @param {{speedThresholdKmh: number}} settings Physical near-stop threshold.
 * @returns {object[]} Stop events with their low-speed intervals.
 */
export function detectStops(input, { speedThresholdKmh }) {
  const threshold = speedThresholdKmh * VIDEO_SYNC_KMH_TO_METERS_PER_SECOND
  const movementThreshold = (speedThresholdKmh + VIDEO_SYNC_STOP_EXIT_HYSTERESIS_KMH) * VIDEO_SYNC_KMH_TO_METERS_PER_SECOND
  const stops = []

  for (const segment of input.segments) {
    let state = createState(STOP_PHASES.UNESTABLISHED)
    let previousIndex = null

    for (let index = segment.startIndex; index < segment.endIndex; index += 1) {
      const speed = input.speed[index]
      if (speed === null) {
        if (previousIndex !== null) extendStopInterval(state, input.elapsedSeconds[previousIndex])
        state = createState(STOP_PHASES.UNESTABLISHED)
        previousIndex = null
        continue
      }

      const time = input.elapsedSeconds[index]
      if (previousIndex === null) {
        state = stateForFirstSample(speed, time, threshold, movementThreshold)
        previousIndex = index
        continue
      }

      const previousTime = input.elapsedSeconds[previousIndex]
      const previousSpeed = input.speed[previousIndex]
      state = advanceState(state, { previousTime, previousSpeed, time, speed, threshold, movementThreshold }, stops)
      previousIndex = index
    }

    if (previousIndex !== null) extendStopInterval(state, input.elapsedSeconds[previousIndex])
  }

  return stops
}
