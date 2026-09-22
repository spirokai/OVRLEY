import {
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS,
  VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
} from '../data/videoSyncConstants'

const MATCHABLE_LANDMARK_TYPES = [
  VIDEO_SYNC_LANDMARK_TYPES.STOP,
  VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN,
  VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN,
  VIDEO_SYNC_LANDMARK_TYPES.LOCATION,
]
const UNAVAILABLE_METRICS = Object.freeze({ speed: false, heading: false, course: false })

/**
 * Derives the landmarks that can participate in matching from the canonical
 * detector availability.
 *
 * @param {object[]} landmarks Canonical video landmarks.
 * @param {{availability: {speed: boolean, heading: boolean, course: boolean}, location: object|null}|null} detection Canonical detected-event model.
 * @returns {{eligibleLandmarks: object[], canMatchAll: boolean, canMatchLocation: boolean}} Eligibility model.
 */
export function getVideoSyncEligibility(landmarks, detection) {
  const eligibleLandmarks = []
  let locationLandmark = null
  const availability = detection?.availability ?? UNAVAILABLE_METRICS
  const location = detection?.location ?? null

  for (const landmark of landmarks) {
    if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) {
      if (location !== null) {
        locationLandmark = landmark
        eligibleLandmarks.push(landmark)
      }
      continue
    }

    const metric = landmark.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? 'speed' : 'heading'
    if (availability[metric]) eligibleLandmarks.push(landmark)
  }

  eligibleLandmarks.sort((left, right) => sortByTime(left, right, (landmark) => landmark.videoSecond))

  return {
    eligibleLandmarks,
    canMatchAll: eligibleLandmarks.length >= 2,
    canMatchLocation: locationLandmark !== null,
  }
}

/**
 * Creates one typed landmark/event offset interval. Detected event periods are
 * equally good throughout their full width; timing uncertainty is represented
 * by the scale used outside that interval.
 *
 * @param {object} landmark Canonical video landmark.
 * @param {object} event Canonical detected activity event.
 * @returns {{landmarkId: string, eventId: string, type: string, event: object, eventSecond: number, startOffset: number, endOffset: number, timingScaleSeconds: number}} Typed offset support.
 */
function createOffsetSupport(landmark, event) {
  const isLocation = landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION
  const isStop = landmark.type === VIDEO_SYNC_LANDMARK_TYPES.STOP
  const eventStart = isLocation ? event.time : isStop ? event.lowSpeedInterval.start : event.start
  const eventEnd = isLocation ? event.time : isStop ? event.lowSpeedInterval.end : event.end
  const startOffset = eventStart - landmark.videoSecond
  const endOffset = eventEnd - landmark.videoSecond

  return {
    landmarkId: landmark.id,
    eventId: event.id,
    type: landmark.type,
    event,
    eventSecond: eventStart,
    startOffset,
    endOffset,
    timingScaleSeconds: isLocation ? VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS : VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
  }
}

/** @param {object} support Offset interval. @param {number} offset Proposed offset. @returns {number} Distance outside the interval. */
export function calculateOffsetResidual(support, offset) {
  if (offset < support.startOffset) return support.startOffset - offset
  if (offset > support.endOffset) return offset - support.endOffset
  return 0
}

/**
 * Sorts canonical activity events or landmarks by their relevant time.
 *
 * @param {{id: string}} left First value.
 * @param {{id: string}} right Second value.
 * @param {(value: object) => number} timeOf Time selector.
 * @returns {number} Ordering result.
 */
function sortByTime(left, right, timeOf) {
  return timeOf(left) - timeOf(right) || left.id.localeCompare(right.id)
}

/**
 * Builds all typed offset supports and the landmarks eligible for matching.
 *
 * @param {object[]} landmarks Canonical video landmarks.
 * @param {{availability: {speed: boolean, heading: boolean}, stops: object[], turns: object[], location: object|null}} detection Canonical detector result.
 * @returns {{eligibleLandmarks: object[], supports: object[]}} Eligible landmarks and typed supports.
 */
export function createVideoSyncOffsetSupports(landmarks, detection) {
  const eventsByType = {
    [VIDEO_SYNC_LANDMARK_TYPES.STOP]: [...detection.stops].sort((left, right) => sortByTime(left, right, (event) => event.lowSpeedInterval.start)),
    [VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN]: [...detection.turns]
      .filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN)
      .sort((left, right) => sortByTime(left, right, (event) => event.start)),
    [VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN]: [...detection.turns]
      .filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN)
      .sort((left, right) => sortByTime(left, right, (event) => event.start)),
  }

  const { eligibleLandmarks } = getVideoSyncEligibility(landmarks, detection)

  const supports = []
  for (const landmark of eligibleLandmarks) {
    if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) {
      supports.push(createOffsetSupport(landmark, detection.location))
      continue
    }
    for (const event of eventsByType[landmark.type]) supports.push(createOffsetSupport(landmark, event))
  }

  return { eligibleLandmarks, supports }
}

export { MATCHABLE_LANDMARK_TYPES }
