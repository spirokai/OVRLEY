import i18next from 'i18next'
import {
  VIDEO_SYNC_KMH_TO_METERS_PER_SECOND,
  VIDEO_SYNC_STOP_ENTRY_DWELL_SECONDS,
  VIDEO_SYNC_STOP_EXIT_DWELL_SECONDS,
  VIDEO_SYNC_STOP_EXIT_HYSTERESIS_KMH,
} from '../data/videoSyncConstants'

/**
 * Validates the user-owned speed threshold at the detector boundary.
 *
 * @param {number} speedThresholdKmh Configured near-stop threshold in km/h.
 * @returns {void}
 * @throws {Error} When the threshold is not finite.
 */
function requireSpeedThreshold(speedThresholdKmh) {
  if (!Number.isFinite(speedThresholdKmh)) {
    throw new Error(i18next.t('videoSync.invalidSpeedThreshold', 'Manual video sync speed threshold must be finite'))
  }
}

/**
 * Interpolates the timestamp at which a sampled value crosses a threshold.
 *
 * @param {number} previousTime Previous sample timestamp.
 * @param {number} previousValue Previous sample value.
 * @param {number} currentTime Current sample timestamp.
 * @param {number} currentValue Current sample value.
 * @param {number} threshold Crossing threshold.
 * @returns {number} Interpolated crossing timestamp.
 */
function interpolateCrossingTime(previousTime, previousValue, currentTime, currentValue, threshold) {
  const valueDelta = currentValue - previousValue
  if (valueDelta === 0) return previousTime

  const ratio = (threshold - previousValue) / valueDelta
  return previousTime + ratio * (currentTime - previousTime)
}

/**
 * Closes an open near-stop suppression interval at a segment boundary.
 *
 * @param {{start: number, end: number}[]} nearStopIntervals Output interval list.
 * @param {number|null} nearStopStart Open interval start, if any.
 * @param {number} endTime Segment or movement end timestamp.
 * @returns {void}
 */
function closeNearStopInterval(nearStopIntervals, nearStopStart, endTime) {
  if (nearStopStart === null) return
  nearStopIntervals.push({ start: nearStopStart, end: Math.max(nearStopStart, endTime) })
}

const STOP_PHASES = Object.freeze({
  UNESTABLISHED: 'unestablished',
  ESTABLISHING_MOVEMENT: 'establishingMovement',
  MOVING: 'moving',
  ENTERING_STOP: 'enteringStop',
  STOPPED: 'stopped',
  EXITING_STOP: 'exitingStop',
})

/**
 * Creates a complete stop-detector state with explicit nullable transition fields.
 *
 * @param {string} phase Current state-machine phase.
 * @param {{movementStartedAt?: number|null, nearStopStartedAt?: number|null, stopCandidateStartedAt?: number|null, exitStartedAt?: number|null, stopEvent?: object|null}} [values] Phase-specific values.
 * @returns {{phase: string, movementStartedAt: number|null, nearStopStartedAt: number|null, stopCandidateStartedAt: number|null, exitStartedAt: number|null, stopEvent: object|null}} Stop-detector state.
 */
function createState(phase, values = {}) {
  return {
    phase,
    movementStartedAt: values.movementStartedAt ?? null,
    nearStopStartedAt: values.nearStopStartedAt ?? null,
    stopCandidateStartedAt: values.stopCandidateStartedAt ?? null,
    exitStartedAt: values.exitStartedAt ?? null,
    stopEvent: values.stopEvent ?? null,
  }
}

/**
 * Closes the state's near-stop interval and any established stop's low-speed interval.
 *
 * @param {{nearStopStartedAt: number|null, stopEvent: object|null}} state Current stop state.
 * @param {number} endTime Interval end timestamp.
 * @param {{start: number, end: number}[]} nearStopIntervals Output interval list.
 * @returns {void}
 */
function closeNearStop(state, endTime, nearStopIntervals) {
  closeNearStopInterval(nearStopIntervals, state.nearStopStartedAt, endTime)
  if (state.stopEvent !== null) {
    state.stopEvent.lowSpeedInterval.end = Math.max(state.stopEvent.lowSpeedInterval.start, endTime)
  }
}

/**
 * Enters the stopped phase while carrying forward an established stop event.
 *
 * @param {{stopEvent: object|null}} previousState State being left.
 * @param {number} nearStopStartedAt Timestamp at which the near-stop interval began.
 * @param {number|null} [stopCandidateStartedAt] Candidate event timestamp.
 * @returns {object} New stopped-phase state.
 */
function enterStopped(previousState, nearStopStartedAt, stopCandidateStartedAt = null) {
  return createState(STOP_PHASES.STOPPED, {
    nearStopStartedAt,
    stopCandidateStartedAt,
    stopEvent: previousState.stopEvent,
  })
}

/**
 * Records a confirmed stop transition and returns the corresponding stopped state.
 *
 * @param {{nearStopStartedAt: number|null, stopCandidateStartedAt: number|null, stopEvent: object|null}} state Candidate stop state.
 * @param {number} time Confirmation timestamp.
 * @param {object[]} stops Output stop-event list.
 * @returns {object} Stopped-phase state containing the new event.
 */
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
  })
}

/**
 * Finalizes an unfinished segment without creating a stop transition.
 *
 * @param {{nearStopStartedAt: number|null, stopEvent: object|null}} state Current stop state.
 * @param {number} endTime Segment end timestamp.
 * @param {{start: number, end: number}[]} nearStopIntervals Output interval list.
 * @returns {void}
 */
function finishSegmentState(state, endTime, nearStopIntervals) {
  if (state.nearStopStartedAt !== null) closeNearStop(state, endTime, nearStopIntervals)
}

/**
 * Seeds the state machine from the first valid speed sample in a segment.
 *
 * @param {number} speed First speed sample in m/s.
 * @param {number} time First sample timestamp.
 * @param {number} threshold Near-stop threshold in m/s.
 * @param {number} movementThreshold Exit/movement threshold in m/s.
 * @returns {object} Initial stop-detector state.
 */
function stateForFirstSample(speed, time, threshold, movementThreshold) {
  if (speed <= threshold) return enterStopped(createState(STOP_PHASES.UNESTABLISHED), time)
  if (speed > movementThreshold) return createState(STOP_PHASES.ESTABLISHING_MOVEMENT, { movementStartedAt: time })
  return createState(STOP_PHASES.UNESTABLISHED)
}

/**
 * Advances the stop state machine across one pair of valid speed samples.
 *
 * The transition loop allows one sample to complete a phase and immediately
 * enter the next phase, while each returned state owns the next comparison.
 *
 * @param {object} state Current stop-detector state.
 * @param {{previousTime: number, previousSpeed: number, time: number, speed: number, threshold: number, movementThreshold: number}} sample Current sample pair and thresholds.
 * @param {object[]} stops Output stop-event list.
 * @param {{start: number, end: number}[]} nearStopIntervals Output suppression interval list.
 * @returns {object} Updated stop-detector state.
 */
function advanceState(state, { previousTime, previousSpeed, time, speed, threshold, movementThreshold }, stops, nearStopIntervals) {
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
        if (speed > movementThreshold) {
          const exitStartedAt =
            previousSpeed <= movementThreshold ? interpolateCrossingTime(previousTime, previousSpeed, time, speed, movementThreshold) : previousTime
          return createState(STOP_PHASES.EXITING_STOP, {
            nearStopStartedAt: nextState.nearStopStartedAt,
            exitStartedAt,
            stopEvent: nextState.stopEvent,
          })
        }
        if (nextState.stopEvent !== null) nextState.stopEvent.lowSpeedInterval.end = time
        return nextState

      case STOP_PHASES.EXITING_STOP:
        if (speed > movementThreshold && time - nextState.exitStartedAt >= VIDEO_SYNC_STOP_EXIT_DWELL_SECONDS) {
          const exitTime = nextState.exitStartedAt
          closeNearStop(nextState, exitTime, nearStopIntervals)
          return createState(STOP_PHASES.MOVING, { movementStartedAt: exitTime })
        }
        if (speed <= movementThreshold) {
          if (nextState.stopEvent !== null) nextState.stopEvent.lowSpeedInterval.end = time
          return createState(STOP_PHASES.STOPPED, {
            nearStopStartedAt: nextState.nearStopStartedAt,
            stopEvent: nextState.stopEvent,
          })
        }
        return nextState

      default:
        throw new Error(i18next.t('videoSync.unknownStopPhase', 'Unknown manual video sync stop phase: {{phase}}', { phase: nextState.phase }))
    }
  }
}

/**
 * Runs the near-stop state machine and retains its suppression intervals for
 * the directional-turn detector. The intervals include stationary starts,
 * while `stops` only contains transitions from established movement.
 *
 * @param {{elapsedSeconds: number[], speed: (number|null)[], segments: {startIndex: number, endIndex: number}[]}} input Detector input produced by createActivitySyncInput.
 * @param {{speedThresholdKmh: number}} settings Physical near-stop threshold.
 * @returns {{stops: object[], nearStopIntervals: {start: number, end: number}[]}}
 */
export function detectStopState(input, { speedThresholdKmh }) {
  requireSpeedThreshold(speedThresholdKmh)

  const threshold = speedThresholdKmh * VIDEO_SYNC_KMH_TO_METERS_PER_SECOND
  const movementThreshold = (speedThresholdKmh + VIDEO_SYNC_STOP_EXIT_HYSTERESIS_KMH) * VIDEO_SYNC_KMH_TO_METERS_PER_SECOND
  const stops = []
  const nearStopIntervals = []

  for (const segment of input.segments) {
    let state = createState(STOP_PHASES.UNESTABLISHED)
    let previousIndex = null

    const finishSegment = () => {
      if (previousIndex === null) return
      const endTime = input.elapsedSeconds[previousIndex]
      finishSegmentState(state, endTime, nearStopIntervals)
    }

    for (let index = segment.startIndex; index < segment.endIndex; index += 1) {
      const speed = input.speed[index]
      if (speed === null) {
        finishSegment()
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
      state = advanceState(state, { previousTime, previousSpeed, time, speed, threshold, movementThreshold }, stops, nearStopIntervals)
      previousIndex = index
    }

    finishSegment()
  }

  return { stops, nearStopIntervals }
}

/**
 * Detects typed near-stop events from timestamped canonical activity data.
 *
 * @param {object} input Detector input produced by createActivitySyncInput.
 * @param {{speedThresholdKmh: number}} settings Physical near-stop threshold.
 * @returns {object[]} Detected stop events.
 */
export function detectStops(input, settings) {
  return detectStopState(input, settings).stops
}
