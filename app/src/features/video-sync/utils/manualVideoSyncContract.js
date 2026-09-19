import {
  VIDEO_SYNC_DEFAULT_SPEED_THRESHOLD_KMH,
  VIDEO_SYNC_DEFAULT_TURN_THRESHOLD_DEGREES,
  VIDEO_SYNC_LANDMARK_TYPES,
} from '../data/videoSyncConstants'

const LANDMARK_TYPE_VALUES = Object.values(VIDEO_SYNC_LANDMARK_TYPES)

/**
 * Requires a non-null, non-array object at a contract boundary.
 *
 * @param {unknown} value Value to inspect.
 * @param {string} label Contract field label.
 * @returns {void}
 * @throws {Error} When the value is not a plain object-shaped value.
 */
function requireObject(value, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`)
  }
}

/**
 * Requires an object to contain exactly the documented keys.
 *
 * @param {object} value Object being checked.
 * @param {string[]} keys Expected keys.
 * @param {string} label Contract field label.
 * @returns {void}
 * @throws {Error} When the object shape differs from the contract.
 */
function requireExactKeys(value, keys, label) {
  const actualKeys = Object.keys(value).sort()
  const expectedKeys = [...keys].sort()
  if (actualKeys.length !== expectedKeys.length || actualKeys.some((key, index) => key !== expectedKeys[index])) {
    throw new Error(`${label} has an invalid shape`)
  }
}

/**
 * Requires a finite numeric contract field.
 *
 * @param {unknown} value Value to inspect.
 * @param {string} label Contract field label.
 * @returns {void}
 * @throws {Error} When the value is not a finite number.
 */
function requireFiniteNumber(value, label) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`${label} must be a finite number`)
  }
}

/**
 * Requires a finite number inside an inclusive range.
 *
 * @param {unknown} value Value to inspect.
 * @param {{min: number, max: number}} range Inclusive range.
 * @param {string} label Contract field label.
 * @returns {void}
 * @throws {Error} When the value is non-numeric or outside the range.
 */
function requireThreshold(value, range, label) {
  requireFiniteNumber(value, label)
  if (value < range.min || value > range.max) {
    throw new Error(`${label} must be between ${range.min} and ${range.max}`)
  }
}

/**
 * Validates a project-local landmark identifier.
 * @param {string} id Landmark identifier.
 * @returns {string} The validated identifier.
 */
export function validateLandmarkId(id) {
  if (typeof id !== 'string' || id.trim() === '') {
    throw new Error('Manual video sync landmark id must be a non-empty string')
  }
  return id
}

/**
 * Validates one physical sensitivity threshold at its commit boundary.
 * @param {number} value Threshold value.
 * @param {{min: number, max: number}} range Inclusive allowed range.
 * @param {string} label Contract field label.
 * @returns {number} The validated threshold.
 */
export function validateThreshold(value, range, label) {
  requireThreshold(value, range, label)
  return value
}

/**
 * Validates one video-local second against the staged video.
 * @param {number} videoSecond Video-local second.
 * @param {number|null} videoDurationSeconds Loaded video duration, if available.
 * @returns {number} The validated video-local second.
 */
export function validateVideoSecond(videoSecond, videoDurationSeconds = null) {
  requireFiniteNumber(videoSecond, 'Manual video sync landmark videoSecond')
  if (videoSecond < 0) {
    throw new Error('Manual video sync landmark videoSecond must not be negative')
  }
  if (videoDurationSeconds !== null) {
    requireFiniteNumber(videoDurationSeconds, 'Imported video duration')
    if (videoDurationSeconds <= 0 || videoSecond > videoDurationSeconds) {
      throw new Error('Manual video sync landmark videoSecond must be within the imported video duration')
    }
  }
  return videoSecond
}

/**
 * Validates an activity-side course location time.
 * @param {number} activitySecond Activity-local second.
 * @returns {number} The validated activity second.
 */
export function validateActivitySecond(activitySecond) {
  requireFiniteNumber(activitySecond, 'Manual video sync detected location activitySecond')
  if (activitySecond < 0) {
    throw new Error('Manual video sync detected location activitySecond must not be negative')
  }
  return activitySecond
}

/**
 * Validates one canonical manual-sync landmark.
 * @param {object} landmark Landmark value at a feature boundary.
 * @param {number|null} videoDurationSeconds Loaded video duration, if available.
 * @returns {object} The validated landmark.
 */
export function validateLandmark(landmark, videoDurationSeconds = null) {
  requireObject(landmark, 'Manual video sync landmark')
  validateLandmarkId(landmark.id)
  if (!LANDMARK_TYPE_VALUES.includes(landmark.type)) {
    throw new Error(`Unsupported manual video sync landmark type: ${String(landmark.type)}`)
  }
  validateVideoSecond(landmark.videoSecond, videoDurationSeconds)

  if (landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION) {
    requireExactKeys(landmark, ['activitySecond', 'id', 'type', 'videoSecond'], 'Location landmark')
    if (landmark.activitySecond !== null) {
      requireFiniteNumber(landmark.activitySecond, 'Location landmark activitySecond')
    }
  } else {
    requireExactKeys(landmark, ['id', 'type', 'videoSecond'], 'Manual video sync landmark')
  }

  return landmark
}

/**
 * Creates the documented durable defaults for a new project.
 * @returns {{landmarks: object[], speedThresholdKmh: number, turnThresholdDegrees: number}} Default state.
 */
export function createDefaultManualState() {
  return {
    landmarks: [],
    speedThresholdKmh: VIDEO_SYNC_DEFAULT_SPEED_THRESHOLD_KMH,
    turnThresholdDegrees: VIDEO_SYNC_DEFAULT_TURN_THRESHOLD_DEGREES,
  }
}

/** @param {object} manualState Validated durable state. @returns {object} A detached state copy. */
export function cloneManualState(manualState) {
  return structuredClone(manualState)
}
