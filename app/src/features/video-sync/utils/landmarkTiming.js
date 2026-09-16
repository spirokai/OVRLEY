import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS } from '../data/videoSyncConstants'

const MATCHABLE_LANDMARK_TYPES = [VIDEO_SYNC_LANDMARK_TYPES.STOP, VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN, VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN]

/**
 * Derives the landmarks that can participate in ordinary matching from the
 * canonical detector availability.
 *
 * @param {object[]} landmarks Canonical video landmarks.
 * @param {{speed: boolean, heading: boolean, course: boolean}} availability Detector metric availability.
 * @returns {{eligibleLandmarks: object[], eligibleLandmarkIds: string[], unsupportedLandmarks: {id: string, type: string, metric: string}[], resolvedLocationLandmark: object|null, canMatchLandmarks: boolean, canUseMapOnly: boolean}} Eligibility model.
 */
export function getVideoSyncEligibility(landmarks, availability) {
  const eligibleLandmarks = []
  const unsupportedLandmarks = []
  let resolvedLocationLandmark = null

  for (const landmark of landmarks) {
    if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) {
      if (landmark.activitySecond !== null) resolvedLocationLandmark = landmark
      continue
    }

    const metric = landmark.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? 'speed' : 'heading'
    if (availability[metric]) {
      eligibleLandmarks.push(landmark)
    } else {
      unsupportedLandmarks.push({ id: landmark.id, type: landmark.type, metric })
    }
  }

  eligibleLandmarks.sort((left, right) => sortByTime(left, right, (landmark) => landmark.videoSecond))

  return {
    eligibleLandmarks,
    eligibleLandmarkIds: eligibleLandmarks.map((landmark) => landmark.id),
    unsupportedLandmarks,
    resolvedLocationLandmark,
    canMatchLandmarks: eligibleLandmarks.length >= 2,
    canUseMapOnly: resolvedLocationLandmark !== null,
  }
}

/**
 * Creates the residual function for one typed landmark/event support interval.
 *
 * @param {object} landmark Canonical video landmark.
 * @param {object} event Canonical detected activity event.
 * @returns {{landmarkId: string, eventId: string, type: string, event: object, startOffset: number, endOffset: number, residualAt: (offset: number) => number, preferredOffsets: number[]}} Typed offset support.
 */
function createOffsetSupport(landmark, event) {
  if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.STOP) {
    const center = event.time - landmark.videoSecond
    return {
      landmarkId: landmark.id,
      eventId: event.id,
      type: landmark.type,
      event,
      startOffset: center - VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
      endOffset: center + VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
      residualAt: (offset) => Math.abs(offset - center),
      preferredOffsets: [center],
    }
  }

  const start = event.start - landmark.videoSecond
  const end = event.end - landmark.videoSecond
  return {
    landmarkId: landmark.id,
    eventId: event.id,
    type: landmark.type,
    event,
    startOffset: start - VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
    endOffset: end + VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
    residualAt: (offset) => {
      if (offset < start) return start - offset
      if (offset > end) return offset - end
      return 0
    },
    preferredOffsets: [start, end],
  }
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
 * Builds all typed offset supports and the landmarks eligible for ordinary
 * stop/turn matching. Location landmarks are intentionally excluded here.
 *
 * @param {object[]} landmarks Canonical video landmarks.
 * @param {{availability: {speed: boolean, heading: boolean}, stops: object[], turns: object[]}} detection Canonical detector result.
 * @returns {{eligibleLandmarks: object[], supports: object[]}} Eligible landmarks and typed supports.
 */
export function createVideoSyncOffsetSupports(landmarks, detection) {
  const eventsByType = {
    [VIDEO_SYNC_LANDMARK_TYPES.STOP]: [...detection.stops].sort((left, right) => sortByTime(left, right, (event) => event.time)),
    [VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN]: [...detection.turns]
      .filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN)
      .sort((left, right) => sortByTime(left, right, (event) => event.start)),
    [VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN]: [...detection.turns]
      .filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN)
      .sort((left, right) => sortByTime(left, right, (event) => event.start)),
  }

  const { eligibleLandmarks } = getVideoSyncEligibility(landmarks, detection.availability)

  const supports = []
  for (const landmark of eligibleLandmarks) {
    for (const event of eventsByType[landmark.type]) supports.push(createOffsetSupport(landmark, event))
  }

  return { eligibleLandmarks, supports }
}

/**
 * Returns whether a support interval contains an offset, including its bounds.
 *
 * @param {{startOffset: number, endOffset: number}} support Typed offset support.
 * @param {number} offset Proposed video offset.
 * @returns {boolean} Whether the offset is supported.
 */
export function offsetIsSupported(support, offset) {
  return offset >= support.startOffset && offset <= support.endOffset
}

/**
 * Returns the public, serializable representation of one offset support.
 *
 * @param {{landmarkId: string, eventId: string, type: string, startOffset: number, endOffset: number}} support Typed offset support.
 * @returns {{landmarkId: string, eventId: string, type: string, startOffset: number, endOffset: number}} Support diagnostic.
 */
export function serializeOffsetSupport(support) {
  return {
    landmarkId: support.landmarkId,
    eventId: support.eventId,
    type: support.type,
    startOffset: support.startOffset,
    endOffset: support.endOffset,
  }
}

export { MATCHABLE_LANDMARK_TYPES }
