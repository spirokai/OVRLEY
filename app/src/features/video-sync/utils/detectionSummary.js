import { VIDEO_SYNC_LANDMARK_TYPES } from '../data/videoSyncConstants'

/**
 * Counts the detected event types for the video-sync drawer summary.
 *
 * @param {object|null} detection Latest detector result, or null before detection runs.
 * @returns {{stops: number, leftTurns: number, rightTurns: number, locations: number}} Detected event counts.
 */
export function getVideoSyncDetectionCounts(detection) {
  if (detection === null) return { stops: 0, leftTurns: 0, rightTurns: 0, locations: 0 }

  return {
    stops: detection.stops.length,
    leftTurns: detection.turns.filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN).length,
    rightTurns: detection.turns.filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN).length,
    locations: detection.location === null ? 0 : 1,
  }
}
