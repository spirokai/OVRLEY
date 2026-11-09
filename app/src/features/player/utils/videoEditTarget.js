/**
 * The clip that cut and exclude commands at the playhead apply to.
 */

import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { resolveEditClip } from '@/lib/video-program/video-segments'

/**
 * Resolves the clip commands at a timeline second apply to (see `resolveEditClip`)
 * and the clip-local second they act at.
 * @param {Array<object>} clips Committed timeline clips.
 * @param {object} program Committed program.
 * @param {string|null} selectedVideoId Selected clip's video, if any.
 * @param {number} second Timeline second.
 * @returns {{clip: object, sourceSecond: number}|null} Target, or null without a clip at the second.
 */
export function resolveEditTarget(clips, program, selectedVideoId, second) {
  const previewVideoId = resolvePreviewTarget(program, second)?.videoId ?? null
  const clip = resolveEditClip(clips, previewVideoId, selectedVideoId, second)
  return clip === null ? null : { clip, sourceSecond: second - clip.offsetSeconds }
}
