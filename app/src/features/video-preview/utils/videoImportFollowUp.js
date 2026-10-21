/**
 * Steps that follow a video import, in the order the user sees them: the import
 * report, a placement prompt per clip whose time could not be determined, and the
 * overlap warning.
 *
 * Whether a step is due is decided when it is reached, so earlier answers (for
 * example a placement that removes an overlap) are taken into account.
 */

import { filenameFromSelectedPath } from '@/lib/utils'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'

/**
 * Creates the follow-up steps of one import.
 * @param {{report: object|null, undeterminedIds: Array<string>, videoIds: Array<string>}} result Import result.
 * @returns {Array<object>} Steps in presentation order, including steps that may turn out not to be due.
 */
export function createImportFollowUpSteps({ report, undeterminedIds, videoIds }) {
  return [
    ...(report === null ? [] : [{ kind: 'report', report }]),
    ...undeterminedIds.map((videoId) => ({ kind: 'placement', videoId })),
    { kind: 'overlaps', videoIds },
  ]
}

/**
 * Lists the clips a clip can be placed next to: every other clip except clips
 * that are still waiting for their own placement, in timeline order.
 * @param {object} state Application state.
 * @param {string} videoId Clip to place.
 * @param {Array<object>} remainingSteps Steps after the current one.
 * @returns {Array<{id: string, name: string}>} Anchor candidates.
 */
export function listPlacementCandidates(state, videoId, remainingSteps) {
  const pending = new Set(remainingSteps.filter((step) => step.kind === 'placement').map((step) => step.videoId))
  return sortVideosByTimeline(joinVideoEdits(state.importedVideos, state.videoEdits))
    .filter((clip) => clip.id !== videoId && !pending.has(clip.id))
    .map((clip) => ({ id: clip.id, name: filenameFromSelectedPath(clip.path) }))
}

/**
 * Lists overlaps on the timeline that involve any of the given clips.
 * @param {object} state Application state.
 * @param {Array<string>} videoIds Clips of the import.
 * @returns {Array<{key: string, earlierName: string, laterName: string, seconds: number}>} Overlaps in timeline order; `seconds`
 *   is rounded to a tenth for display.
 */
export function listImportOverlaps(state, videoIds) {
  const ids = new Set(videoIds)
  const { program } = buildTimelineProgram({
    videos: state.importedVideos,
    videoEdits: state.videoEdits,
    dragPreview: null,
    transitionDefaults: state.transitionDefaults,
  })
  const nameOf = (videoId) => filenameFromSelectedPath(state.importedVideos.find((video) => video.id === videoId).path)
  return program.overlaps
    .filter((overlap) => ids.has(overlap.earlierVideoId) || ids.has(overlap.laterVideoId))
    .map((overlap) => ({
      key: `${overlap.earlierVideoId}:${overlap.laterVideoId}:${overlap.startSecond}`,
      earlierName: nameOf(overlap.earlierVideoId),
      laterName: nameOf(overlap.laterVideoId),
      seconds: Math.round((overlap.endSecond - overlap.startSecond) * 10) / 10,
    }))
}

function isStepDue(step, remainingSteps, state) {
  switch (step.kind) {
    case 'report':
      return true
    case 'placement':
      return (
        state.importedVideos.some((video) => video.id === step.videoId) &&
        state.videoEdits[step.videoId].placementAnchor === null &&
        listPlacementCandidates(state, step.videoId, remainingSteps).length > 0
      )
    case 'overlaps':
      return listImportOverlaps(state, step.videoIds).length > 0
    default:
      throw new Error(`Unknown import follow-up step: ${step.kind}`)
  }
}

/**
 * Drops leading steps that are not due in the current state.
 * @param {Array<object>} steps Remaining steps.
 * @param {object} state Application state.
 * @returns {Array<object>} Steps starting with the next due step, or empty.
 */
export function dropSettledSteps(steps, state) {
  const index = steps.findIndex((step, position) => isStepDue(step, steps.slice(position + 1), state))
  return index === -1 ? [] : steps.slice(index)
}
