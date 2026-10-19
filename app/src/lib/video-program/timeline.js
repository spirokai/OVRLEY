/**
 * The editor timeline program: every imported video, unrestricted by the export range.
 */

import { buildProgram } from './program'
import { applyVideoDragPreview, getVideosTimelineBounds, joinVideoEdits } from './video-collection'

export const FULL_EXPORT_RANGE = Object.freeze({ type: 'all', from: 0, to: 0 })

/**
 * Builds the clips and program shown on the editor timeline, including a transient clip drag.
 * @param {object} options Timeline inputs.
 * @param {Array<object>} options.videos Imported video entities.
 * @param {object} options.videoEdits User edits keyed by video ID.
 * @param {{videoId: string|null, deltaSeconds: number}|null} options.dragPreview Transient drag shift.
 * @param {object} options.transitionDefaults Project transition defaults.
 * @returns {{clips: Array<object>, program: object, bounds: {startSecond: number, endSecond: number}|null}} Displayed clips, program, and covered interval.
 */
export function buildTimelineProgram({ videos, videoEdits, dragPreview, transitionDefaults }) {
  const clips = applyVideoDragPreview(joinVideoEdits(videos, videoEdits), dragPreview)
  return {
    clips,
    program: buildProgram({ videos: clips, exportRange: FULL_EXPORT_RANGE, transitionDefaults }),
    bounds: getVideosTimelineBounds(clips),
  }
}
