/**
 * Pure helpers over the imported-video collection.
 *
 * Videos are canonical entities validated at import ingress: `offsetSeconds`
 * is the committed timeline start, `durationSeconds` is frame-exact, `fps` is
 * an exact rational, and `resolution` is display-oriented.
 */

import { fpsEquals } from './frame-grid'

/**
 * Returns the timeline second where a video ends.
 * @param {{offsetSeconds: number, durationSeconds: number}} video Imported video.
 * @returns {number} Exclusive timeline end second.
 */
export function getVideoEndSecond(video) {
  return video.offsetSeconds + video.durationSeconds
}

/**
 * Orders videos by committed timeline start, then by import order.
 * @param {Array<object>} videos Imported videos.
 * @returns {Array<object>} New array in timeline order.
 */
export function sortVideosByTimeline(videos) {
  return [...videos].sort((left, right) => left.offsetSeconds - right.offsetSeconds || left.importOrder - right.importOrder)
}

/**
 * Returns the timeline interval covered by all videos.
 * @param {Array<object>} videos Imported videos.
 * @returns {{startSecond: number, endSecond: number}|null} Covered interval, or null without videos.
 */
export function getVideosTimelineBounds(videos) {
  if (videos.length === 0) return null
  return {
    startSecond: Math.min(...videos.map((video) => video.offsetSeconds)),
    endSecond: Math.max(...videos.map(getVideoEndSecond)),
  }
}

function sameFormat(left, right) {
  return left.resolution.width === right.resolution.width && left.resolution.height === right.resolution.height && fpsEquals(left.fps, right.fps)
}

/**
 * Groups videos by display resolution and exact frame rate. The largest group is
 * compatible; ties go to the group containing the earliest-imported video.
 * Single-file export requires every video to be compatible.
 * @param {Array<object>} videos Imported videos.
 * @returns {{reference: {resolution: {width: number, height: number}, fps: {num: number, den: number}}|null, incompatibleIds: Set<string>}} Compatibility result.
 */
export function deriveCompatibility(videos) {
  const groups = []
  for (const video of [...videos].sort((left, right) => left.importOrder - right.importOrder)) {
    const group = groups.find((candidate) => sameFormat(candidate[0], video))
    if (group) group.push(video)
    else groups.push([video])
  }
  if (groups.length === 0) return { reference: null, incompatibleIds: new Set() }

  const reference = groups.reduce((best, group) => (group.length > best.length ? group : best))
  return {
    reference: { resolution: { ...reference[0].resolution }, fps: { ...reference[0].fps } },
    incompatibleIds: new Set(videos.filter((video) => !reference.includes(video)).map((video) => video.id)),
  }
}
