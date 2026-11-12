/**
 * The editor timeline program of the imported videos.
 */

import { buildProgram } from './program'
import { applyVideoDragPreview, getVideosTimelineBounds, joinVideoEdits } from './video-collection'

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
    program: buildProgram({ videos: clips, transitionDefaults }),
    bounds: getVideosTimelineBounds(clips),
  }
}

/** Remembers the latest result, so every subscriber shares one build per change. */
function memoizeLatest(build) {
  let latestArgs = null
  let latestValue
  return (...args) => {
    if (latestArgs === null || args.some((arg, index) => arg !== latestArgs[index])) {
      latestArgs = args
      latestValue = build(...args)
    }
    return latestValue
  }
}

const buildCommitted = memoizeLatest((videos, videoEdits, transitionDefaults) =>
  buildTimelineProgram({ videos, videoEdits, dragPreview: null, transitionDefaults }),
)
const buildDisplayed = memoizeLatest((committed, videos, videoEdits, dragPreview, transitionDefaults) =>
  dragPreview === null ? committed : buildTimelineProgram({ videos, videoEdits, dragPreview, transitionDefaults }),
)
const combineTimeline = memoizeLatest((committed, displayed) => ({ ...committed, displayed }))

/**
 * Selects the committed clips, their timeline program, and covered interval, plus
 * the same values as displayed during a transient clip drag.
 * @param {object} state Application state.
 * @returns {{clips: Array<object>, program: object, bounds: object|null, displayed: {clips: Array<object>, program: object, bounds: object|null}}} Timeline.
 */
export function selectVideoTimeline({ importedVideos, videoEdits, videoDragPreview, transitionDefaults }) {
  const committed = buildCommitted(importedVideos, videoEdits, transitionDefaults)
  return combineTimeline(committed, buildDisplayed(committed, importedVideos, videoEdits, videoDragPreview, transitionDefaults))
}
