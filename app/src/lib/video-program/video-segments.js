/**
 * Pure helpers for the virtual clips of a video. Cuts `{ id, frame }` split a video
 * at integer frames (strictly increasing, inside the video). Each resulting segment
 * has edits `{ excluded, transitionIn, skipOverlay }` in `segments`, keyed by the cut it
 * starts at, or `FIRST_SEGMENT_KEY` for the first one. `skipOverlay` hides the activity
 * widgets over the segment, in the preview and the render. A video without cuts is one segment,
 * so excluding a whole video and excluding a virtual clip are the same operation.
 */

import { secondsToFrames } from './frame-grid'

export const FIRST_SEGMENT_KEY = 'start'

/**
 * Lists a video's segments in source order.
 * @param {{cuts: Array<{id: string, frame: number}>, segments: object, frameCount: number}} video Timeline clip.
 * @returns {Array<{key: string, startFrame: number, endFrame: number, excluded: boolean, transitionIn: object|null, skipOverlay: boolean}>} Segments.
 */
export function listVideoSegments(video) {
  const starts = [{ key: FIRST_SEGMENT_KEY, frame: 0 }, ...video.cuts.map((cut) => ({ key: cut.id, frame: cut.frame }))]
  return starts.map((start, index) => ({
    key: start.key,
    startFrame: start.frame,
    endFrame: starts[index + 1]?.frame ?? video.frameCount,
    ...video.segments[start.key],
  }))
}

/**
 * Copies the segment edits of a video (see the module description).
 * @param {Object<string, {excluded: boolean, transitionIn: {type: string, durationSeconds: number}|null, skipOverlay: boolean}>} segments Segment edits.
 * @returns {Object<string, {excluded: boolean, transitionIn: {type: string, durationSeconds: number}|null, skipOverlay: boolean}>} Independent copy.
 */
export function cloneVideoSegments(segments) {
  return Object.fromEntries(
    Object.entries(segments).map(([key, { excluded, transitionIn, skipOverlay }]) => [
      key,
      {
        excluded,
        transitionIn: transitionIn === null ? null : { type: transitionIn.type, durationSeconds: transitionIn.durationSeconds },
        skipOverlay,
      },
    ]),
  )
}

/**
 * Fits a video's cuts to a source file of `frameCount` frames: cuts at or past its
 * end are dropped with the edits of the segments they start.
 * @param {{cuts: Array<{id: string, frame: number}>, segments: object}} edits Video edits.
 * @param {number} frameCount Frames of the source file.
 * @returns {{cuts: Array<{id: string, frame: number}>, segments: object}} Independent copies of the cuts and segment edits that fit.
 */
export function fitCutsToFrameCount({ cuts, segments }, frameCount) {
  const kept = cuts.filter((cut) => cut.frame < frameCount)
  const keys = new Set([FIRST_SEGMENT_KEY, ...kept.map((cut) => cut.id)])
  return {
    cuts: kept.map((cut) => ({ id: cut.id, frame: cut.frame })),
    segments: cloneVideoSegments(Object.fromEntries(Object.entries(segments).filter(([key]) => keys.has(key)))),
  }
}

/**
 * Returns the key of the join into a segment.
 * @param {string} videoId Video of the segment.
 * @param {string} segmentKey Segment key.
 * @returns {string} Join key, unique across the program.
 */
export function segmentJoinKey(videoId, segmentKey) {
  return `${videoId}:${segmentKey}`
}

/**
 * Whether every segment of a video is excluded from export.
 * @param {{segments: object}} video Timeline clip.
 * @returns {boolean} Whether nothing of the video is exported.
 */
export function isVideoExcluded(video) {
  return Object.values(video.segments).every((segment) => segment.excluded)
}

/**
 * Returns the frame a cut at a source second would use, when one can go there.
 * @param {object} video Timeline clip.
 * @param {number} sourceSecond Clip-local second.
 * @returns {number|null} Frame strictly inside the video and not on a cut, or null.
 */
export function resolveCutFrame(video, sourceSecond) {
  const frame = secondsToFrames(sourceSecond, video.fps)
  if (frame <= 0 || frame >= video.frameCount || video.cuts.some((cut) => cut.frame === frame)) return null
  return frame
}

/**
 * Returns the segment showing at a source second. A second at the video's end
 * belongs to its last segment.
 * @param {object} video Timeline clip.
 * @param {number} sourceSecond Clip-local second.
 * @returns {object} Segment (see `listVideoSegments`).
 */
export function findSegmentAt(video, sourceSecond) {
  const frame = Math.min(secondsToFrames(sourceSecond, video.fps), video.frameCount - 1)
  return listVideoSegments(video).find((segment) => frame < segment.endFrame)
}

/**
 * Picks the clip that cut and exclude commands at a timeline second apply to: the
 * selected clip when it covers the second, otherwise the clip the preview shows
 * there when it covers the second, otherwise the earliest clip covering it.
 * @param {Array<object>} clips Timeline clips.
 * @param {string|null} previewVideoId Video the preview shows at the second.
 * @param {string|null} selectedVideoId Selected clip, if any.
 * @param {number} second Timeline second.
 * @returns {object|null} Clip covering the second, or null.
 */
export function resolveEditClip(clips, previewVideoId, selectedVideoId, second) {
  const covering = clips.filter((clip) => clip.offsetSeconds <= second && second <= clip.offsetSeconds + clip.durationSeconds)
  return (
    covering.find((clip) => clip.id === selectedVideoId) ??
    covering.find((clip) => clip.id === previewVideoId) ??
    [...covering].sort((left, right) => left.offsetSeconds - right.offsetSeconds || left.importOrder - right.importOrder)[0] ??
    null
  )
}
