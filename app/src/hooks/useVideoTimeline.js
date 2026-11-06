/**
 * Store-backed derivations of the imported-video timeline.
 */

import { useCallback, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { hasActivityCoverage, resolveActivityClipId, resolveActivityContext, resolveAvailableMetrics } from '@/lib/activity/activity-context'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { listTransitionProxyJobs } from '@/lib/video-program/transition-proxies'
import { deriveVideoWarnings } from '@/lib/video-program/video-warnings'

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

/**
 * Returns the timeline (see `selectVideoTimeline`).
 * @returns {object} Timeline, built once per change for every subscriber.
 */
export function useVideoTimeline() {
  return useStore(selectVideoTimeline)
}

/**
 * Returns the warnings of every committed clip (see `deriveVideoWarnings`).
 * @returns {Map<string, Array<{kind: string, params: object}>>} Warnings keyed by video ID.
 */
export function useVideoWarnings() {
  const { clips, program } = useVideoTimeline()
  const activityDurationSeconds = useStore((state) => (state.parsedActivity === null ? null : state.activitySummary.durationSeconds))
  return useMemo(() => deriveVideoWarnings(clips, program, activityDurationSeconds), [activityDurationSeconds, clips, program])
}

/**
 * Returns the activity that drives widgets at the playhead. Consumers re-render
 * only when that activity changes, not on every playhead frame.
 * @returns {{activity: object|null, summary: object|null, originSecond: number, videoId: string|null}|null} Activity context.
 */
export function useActivityContext() {
  const parsedActivity = useStore((state) => state.parsedActivity)
  const activitySummary = useStore((state) => state.activitySummary)
  const { clips, program } = useVideoTimeline().displayed
  const clipId = useStore(
    useCallback((state) => (parsedActivity === null ? resolveActivityClipId(program, state.selectedSecond) : null), [parsedActivity, program]),
  )
  return useMemo(
    () =>
      resolveActivityContext({
        parsedActivity,
        activitySummary,
        clip: clipId === null ? null : clips.find((clip) => clip.id === clipId),
      }),
    [activitySummary, clipId, clips, parsedActivity],
  )
}

/**
 * Returns whether activity data covers the playhead (see `hasActivityCoverage`).
 * Consumers re-render only when coverage changes, not on every playhead frame.
 * @returns {boolean} Whether non-raster widgets are shown at the playhead.
 */
export function usePlayheadActivityCoverage() {
  const parsedActivity = useStore((state) => state.parsedActivity)
  const activitySummary = useStore((state) => state.activitySummary)
  const { clips, program } = useVideoTimeline().displayed
  return useStore(
    useCallback(
      (state) => hasActivityCoverage({ parsedActivity, activitySummary, program, clips, second: state.selectedSecond }),
      [activitySummary, clips, parsedActivity, program],
    ),
  )
}

/**
 * Returns metrics available for new widgets across the external activity or all embedded telemetry.
 * @returns {Array<{attribute: string, source: string}>} Available metrics.
 */
export function useAvailableMetrics() {
  const { activitySummary, importedVideos } = useStore(
    useShallow((state) => ({ activitySummary: state.activitySummary, importedVideos: state.importedVideos })),
  )
  return useMemo(() => resolveAvailableMetrics(activitySummary, importedVideos), [activitySummary, importedVideos])
}

/**
 * Returns the transition proxy jobs of the committed program.
 * @returns {Array<object>} Jobs (see `listTransitionProxyJobs`).
 */
export function useTransitionProxyJobs() {
  const { clips, program } = useVideoTimeline()
  return useMemo(() => listTransitionProxyJobs(program, clips), [clips, program])
}
