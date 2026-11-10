/**
 * Pure helpers of a cut being dragged along the timeline.
 */

/**
 * Clamps a frame to the frames a cut can move to: strictly between its neighbouring cuts (or the video's ends).
 * @param {{cuts: Array<{id: string, frame: number}>, frameCount: number}} clip Timeline clip.
 * @param {string} cutId Dragged cut.
 * @param {number} frame Proposed integer frame.
 * @returns {number} Legal frame.
 */
export function clampCutFrame(clip, cutId, frame) {
  const index = clip.cuts.findIndex((cut) => cut.id === cutId)
  const lower = index === 0 ? 0 : clip.cuts[index - 1].frame
  const upper = clip.cuts[index + 1]?.frame ?? clip.frameCount
  return Math.min(upper - 1, Math.max(lower + 1, frame))
}

/**
 * Returns the clips with a dragged cut at its draft frame.
 * @param {Array<object>} clips Timeline clips.
 * @param {{videoId: string, cutId: string, frame: number}|null} draft Dragged cut, or null.
 * @returns {Array<object>} Clips to display.
 */
export function applyCutDraft(clips, draft) {
  if (draft === null) return clips
  return clips.map((clip) =>
    clip.id === draft.videoId ? { ...clip, cuts: clip.cuts.map((cut) => (cut.id === draft.cutId ? { ...cut, frame: draft.frame } : cut)) } : clip,
  )
}
