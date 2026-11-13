/**
 * Pure helpers of a cut being dragged along the timeline. A clip's edge whose outer
 * part is still exported is an edge cut: a cut that does not exist yet, at frame 0
 * or at the last frame. Dragging it in trims that end of the video.
 */

import { segmentBeforeCut } from '@/lib/video-program/collection-edits'
import { framesToSeconds } from '@/lib/video-program/frame-grid'
import { FIRST_SEGMENT_KEY } from '@/lib/video-program/video-segments'
import { videoLaneId } from './timelineGeometry'

const EDGE_CUT_IDS = { start: 'edge-start', end: 'edge-end' }

/**
 * Returns which end of the video an edge cut trims.
 * @param {string} cutId Cut ID.
 * @returns {'start'|'end'|null} Trimmed end, or null for a real cut.
 */
export function edgeCutSide(cutId) {
  if (cutId === EDGE_CUT_IDS.start) return 'start'
  if (cutId === EDGE_CUT_IDS.end) return 'end'
  return null
}

/**
 * Returns the segment a pressed cut belongs to, selected when the press starts with
 * no selection on its clip: the part an edge cut trims into, or the part before a cut.
 * @param {{cuts: Array<{id: string}>}} clip Timeline clip.
 * @param {string} cutId Pressed cut, edge cuts included.
 * @returns {string} Segment key.
 */
export function segmentOfCut(clip, cutId) {
  const side = edgeCutSide(cutId)
  if (side === 'start') return FIRST_SEGMENT_KEY
  if (side === 'end') return clip.cuts.at(-1)?.id ?? FIRST_SEGMENT_KEY
  return segmentBeforeCut(clip, cutId)
}

/**
 * Returns the selection to display while a cut is dragged: a dragged start edge cut
 * hands the clip's first segment to the part it now starts, so a selected first
 * segment follows it.
 * @param {{laneId: string, segmentKey: string|null}|null} selection Clip selection.
 * @param {{videoId: string, cutId: string}|null} draft Dragged cut, or null.
 * @returns {{laneId: string, segmentKey: string|null}|null} Selection to display.
 */
export function applyCutDraftToSelection(selection, draft) {
  if (draft === null || edgeCutSide(draft.cutId) !== 'start') return selection
  const laneId = videoLaneId(draft.videoId)
  if (selection?.laneId !== laneId || selection.segmentKey !== FIRST_SEGMENT_KEY) return selection
  return { laneId, segmentKey: draft.cutId }
}

/**
 * Lists the edge cuts of a clip: one per end whose outer part is still exported.
 * @param {{cuts: Array<{id: string, frame: number}>, segments: object, frameCount: number}} clip Timeline clip.
 * @returns {Array<{id: string, frame: number}>} Edge cuts.
 */
export function listEdgeCuts(clip) {
  const lastKey = clip.cuts.at(-1)?.id ?? FIRST_SEGMENT_KEY
  return [
    ...(clip.segments[FIRST_SEGMENT_KEY].excluded ? [] : [{ id: EDGE_CUT_IDS.start, frame: 0 }]),
    ...(clip.segments[lastKey].excluded ? [] : [{ id: EDGE_CUT_IDS.end, frame: clip.frameCount }]),
  ]
}

/** Frames a real cut may be dragged between: its neighbouring cuts, or the video's ends. */
function cutBounds(clip, cutId) {
  const index = clip.cuts.findIndex((cut) => cut.id === cutId)
  return { lower: index === 0 ? 0 : clip.cuts[index - 1].frame, upper: clip.cuts[index + 1]?.frame ?? clip.frameCount }
}

/**
 * Returns the part kept when a cut is dropped where it closes up the part on one side
 * of it, on its neighbouring cut or the video's end: that part goes, and the part on
 * the other side of the cut grows over it.
 * @param {{cuts: Array<{id: string, frame: number}>, frameCount: number}} clip Timeline clip.
 * @param {string} cutId Dropped cut.
 * @param {number} frame Frame it was dropped on.
 * @returns {string|null} Key of the kept segment, or null when the cut just moves.
 */
export function segmentKeptByDroppedCut(clip, cutId, frame) {
  if (edgeCutSide(cutId) !== null) return null
  const { lower, upper } = cutBounds(clip, cutId)
  if (frame <= lower) return cutId
  if (frame >= upper) return segmentBeforeCut(clip, cutId)
  return null
}

/**
 * Timeline seconds a dragged cut snaps to: its video's ends, the video's other cuts,
 * and, as for a dragged clip, the ends of the other videos and of the activity. An
 * edge cut does not snap to its video's cuts, which it stops before.
 * @param {{id: string, offsetSeconds: number, durationSeconds: number, fps: number, cuts: Array<{id: string, frame: number}>}} clip Dragged cut's clip.
 * @param {string} cutId Dragged cut.
 * @param {{clips: Array<{id: string, offsetSeconds: number, durationSeconds: number}>, activityDurationSeconds: number}} targets
 *   Timeline clips, and the activity duration (0 without an activity).
 * @returns {number[]} Snap targets in timeline seconds.
 */
export function cutSnapEdges(clip, cutId, { clips, activityDurationSeconds }) {
  const ownCuts = edgeCutSide(cutId) !== null ? [] : clip.cuts.filter((cut) => cut.id !== cutId)
  return [
    clip.offsetSeconds,
    clip.offsetSeconds + clip.durationSeconds,
    ...ownCuts.map((cut) => clip.offsetSeconds + framesToSeconds(cut.frame, clip.fps)),
    ...clips.filter((other) => other.id !== clip.id).flatMap((other) => [other.offsetSeconds, other.offsetSeconds + other.durationSeconds]),
    ...(activityDurationSeconds > 0 ? [0, activityDurationSeconds] : []),
  ]
}

/**
 * Resolves the frame a dragged cut goes to. A cut stays between its neighbouring cuts
 * (or the video's ends) and may reach them, where dropping it removes the part it
 * closes up (`segmentKeptByDroppedCut`). An edge cut stops before the nearest cut,
 * and dragging it back past its edge leaves no trim.
 * @param {{cuts: Array<{id: string, frame: number}>, frameCount: number}} clip Timeline clip, possibly showing the dragged cut's draft.
 * @param {string} cutId Dragged cut.
 * @param {number} frame Proposed integer frame.
 * @returns {number|null} Legal frame, or null for no trim.
 */
export function clampCutFrame(clip, cutId, frame) {
  const side = edgeCutSide(cutId)
  if (side !== null) {
    const cuts = clip.cuts.filter((cut) => cut.id !== cutId)
    if (side === 'start') return frame <= 0 ? null : Math.min(frame, (cuts[0]?.frame ?? clip.frameCount) - 1)
    return frame >= clip.frameCount ? null : Math.max(frame, (cuts.at(-1)?.frame ?? 0) + 1)
  }
  const { lower, upper } = cutBounds(clip, cutId)
  return Math.min(upper, Math.max(lower, frame))
}

function applyEdgeDraft(clip, side, { cutId, frame }) {
  if (side === 'start') {
    return {
      ...clip,
      cuts: [{ id: cutId, frame }, ...clip.cuts],
      segments: {
        ...clip.segments,
        [FIRST_SEGMENT_KEY]: { ...clip.segments[FIRST_SEGMENT_KEY], excluded: true, transitionIn: null },
        [cutId]: clip.segments[FIRST_SEGMENT_KEY],
      },
    }
  }
  return {
    ...clip,
    cuts: [...clip.cuts, { id: cutId, frame }],
    segments: { ...clip.segments, [cutId]: { ...clip.segments[clip.cuts.at(-1)?.id ?? FIRST_SEGMENT_KEY], excluded: true, transitionIn: null } },
  }
}

/**
 * Returns the clips with a dragged cut at its draft frame; a dragged edge cut shows
 * as a cut with the outer part excluded.
 * @param {Array<object>} clips Timeline clips.
 * @param {{videoId: string, cutId: string, frame: number}|null} draft Dragged cut, or null.
 * @returns {Array<object>} Clips to display.
 */
export function applyCutDraft(clips, draft) {
  if (draft === null) return clips
  const side = edgeCutSide(draft.cutId)
  return clips.map((clip) => {
    if (clip.id !== draft.videoId) return clip
    if (side !== null) return applyEdgeDraft(clip, side, draft)
    return { ...clip, cuts: clip.cuts.map((cut) => (cut.id === draft.cutId ? { ...cut, frame: draft.frame } : cut)) }
  })
}
