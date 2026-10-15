/**
 * Pure helpers over the imported-video collection.
 *
 * Timeline clips join a canonical video entity (validated at import ingress:
 * `durationSeconds` is frame-exact, `fps` is an exact rational, `resolution` is
 * display-oriented) with its user edits (`offsetSeconds` committed timeline
 * start, `deletedParts`, `transitionIn`).
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

/**
 * Joins imported video entities with their user edits into timeline clips.
 * @param {Array<object>} videos Imported video entities.
 * @param {Object<string, {offsetSeconds: number, deletedParts: Array<object>, transitionIn: object|null}>} videoEdits Edits keyed by video ID.
 * @returns {Array<object>} Timeline clips.
 */
export function joinVideoEdits(videos, videoEdits) {
  return videos.map((video) => ({ ...video, ...videoEdits[video.id] }))
}

/**
 * Returns clips with a transient drag shift applied to one clip, or to every clip when `videoId` is null.
 * @param {Array<object>} clips Timeline clips.
 * @param {{videoId: string|null, deltaSeconds: number}|null} dragPreview Transient drag shift.
 * @returns {Array<object>} Clips as displayed during the drag.
 */
export function applyVideoDragPreview(clips, dragPreview) {
  if (dragPreview === null) return clips
  return clips.map((clip) =>
    dragPreview.videoId === null || clip.id === dragPreview.videoId
      ? { ...clip, offsetSeconds: clip.offsetSeconds + dragPreview.deltaSeconds }
      : clip,
  )
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
