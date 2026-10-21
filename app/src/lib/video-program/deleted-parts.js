/**
 * Pure helpers for deleted parts: clip-local source ranges `{ id, fromSecond,
 * toSecond, transition }` excluded from every output. Ranges lie on the clip's
 * exact frame grid and never overlap or touch; a `transition` of `null` follows
 * the project transition defaults at the join the part creates.
 */

import { framesToSeconds, secondsToFrames } from './frame-grid'

/**
 * Snaps a clip-local range to the clip's frame grid.
 * @param {{fps: {num: number, den: number}, frameCount: number}} video Imported video.
 * @param {number} fromSecond Range start in source seconds.
 * @param {number} toSecond Range end in source seconds.
 * @returns {{fromSecond: number, toSecond: number}} Frame-exact range.
 */
export function snapDeletedRange(video, fromSecond, toSecond) {
  if (!Number.isFinite(fromSecond) || !Number.isFinite(toSecond)) throw new Error('Deleted part bounds must be finite numbers')
  const startFrame = secondsToFrames(fromSecond, video.fps)
  const endFrame = secondsToFrames(toSecond, video.fps)
  if (startFrame < 0 || endFrame > video.frameCount || startFrame >= endFrame) {
    throw new Error('A deleted part must cover at least one frame inside its video')
  }
  return { fromSecond: framesToSeconds(startFrame, video.fps), toSecond: framesToSeconds(endFrame, video.fps) }
}

/**
 * Whether a clip-local range covers at least one frame inside the video.
 * @param {{fps: {num: number, den: number}, frameCount: number}} video Imported video.
 * @param {number} fromSecond Range start in source seconds.
 * @param {number} toSecond Range end in source seconds.
 * @returns {boolean} Whether `snapDeletedRange` accepts the range.
 */
export function isDeletableRange(video, fromSecond, toSecond) {
  if (!Number.isFinite(fromSecond) || !Number.isFinite(toSecond)) return false
  const startFrame = secondsToFrames(fromSecond, video.fps)
  const endFrame = secondsToFrames(toSecond, video.fps)
  return startFrame >= 0 && endFrame <= video.frameCount && startFrame < endFrame
}

/**
 * Adds a frame-exact part to a video's parts, merging every part it overlaps or
 * touches into one. The merged part keeps the identity and transition of the
 * earliest existing part it absorbs; a part absorbing none keeps its own.
 * @param {Array<object>} parts Existing deleted parts of the video.
 * @param {{id: string, fromSecond: number, toSecond: number, transition: object|null}} part Part to add.
 * @returns {Array<object>} Parts in source order.
 */
export function mergeDeletedPart(parts, part) {
  const absorbed = parts.filter((existing) => existing.fromSecond <= part.toSecond && existing.toSecond >= part.fromSecond)
  const kept = parts.filter((existing) => !absorbed.includes(existing))
  const [owner] = [...absorbed].sort((left, right) => left.fromSecond - right.fromSecond)
  const merged = {
    id: owner?.id ?? part.id,
    fromSecond: Math.min(part.fromSecond, ...absorbed.map((existing) => existing.fromSecond)),
    toSecond: Math.max(part.toSecond, ...absorbed.map((existing) => existing.toSecond)),
    transition: owner === undefined ? part.transition : owner.transition,
  }
  return [...kept, merged].sort((left, right) => left.fromSecond - right.fromSecond)
}

/**
 * Picks the clip that deleted-part marks at a timeline second apply to: the
 * preferred clip when it covers the second, otherwise the clip the preview shows
 * there when it covers the second, otherwise the earliest clip covering it.
 * @param {Array<object>} clips Timeline clips (videos joined with their edits).
 * @param {string|null} previewVideoId Video the preview shows at the second.
 * @param {string|null} preferredVideoId Selected clip, if any.
 * @param {number} second Timeline second.
 * @returns {object|null} Clip covering the second, or null.
 */
export function resolveMarkedClip(clips, previewVideoId, preferredVideoId, second) {
  const covering = clips.filter((clip) => clip.offsetSeconds <= second && second <= clip.offsetSeconds + clip.durationSeconds)
  return (
    covering.find((clip) => clip.id === preferredVideoId) ??
    covering.find((clip) => clip.id === previewVideoId) ??
    [...covering].sort((left, right) => left.offsetSeconds - right.offsetSeconds || left.importOrder - right.importOrder)[0] ??
    null
  )
}
