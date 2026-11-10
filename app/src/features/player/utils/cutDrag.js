/**
 * Pure helpers of a cut being dragged along the timeline. A clip's edge whose outer
 * part is still exported is an edge cut: a cut that does not exist yet, at frame 0
 * or at the last frame. Dragging it in trims that end of the video.
 */

import { FIRST_SEGMENT_KEY } from '@/lib/video-program/video-segments'

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

/**
 * Resolves the frame a dragged cut goes to. A cut stays strictly between its
 * neighbouring cuts (or the video's ends). An edge cut stops before the nearest cut,
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
  const index = clip.cuts.findIndex((cut) => cut.id === cutId)
  const lower = index === 0 ? 0 : clip.cuts[index - 1].frame
  const upper = clip.cuts[index + 1]?.frame ?? clip.frameCount
  return Math.min(upper - 1, Math.max(lower + 1, frame))
}

function applyEdgeDraft(clip, side, { cutId, frame }) {
  if (side === 'start') {
    return {
      ...clip,
      cuts: [{ id: cutId, frame }, ...clip.cuts],
      segments: { ...clip.segments, [FIRST_SEGMENT_KEY]: { excluded: true, transitionIn: null }, [cutId]: clip.segments[FIRST_SEGMENT_KEY] },
    }
  }
  return { ...clip, cuts: [...clip.cuts, { id: cutId, frame }], segments: { ...clip.segments, [cutId]: { excluded: true, transitionIn: null } } }
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
