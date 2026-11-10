/**
 * The clip that cut and exclude commands at the playhead apply to.
 */

import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { listVideoSegments, resolveEditClip } from '@/lib/video-program/video-segments'
import { videoIdFromLaneId } from './timelineGeometry'

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

/**
 * Resolves the segment selected on the timeline. A selection without a segment
 * (the activity clip, or a clip selected from the keyboard) has none, and so has a
 * selection whose segment a removed cut merged away.
 * @param {Array<object>} clips Committed timeline clips.
 * @param {{laneId: string, segmentKey: string|null}|null} selectedClip Timeline selection.
 * @returns {{clip: object, segment: object}|null} Selected segment, or null.
 */
export function resolveSelectedSegment(clips, selectedClip) {
  if (selectedClip === null || selectedClip.segmentKey === null) return null
  const clip = clips.find((candidate) => candidate.id === videoIdFromLaneId(selectedClip.laneId))
  const segment = clip === undefined ? undefined : listVideoSegments(clip).find((candidate) => candidate.key === selectedClip.segmentKey)
  return segment === undefined ? null : { clip, segment }
}
