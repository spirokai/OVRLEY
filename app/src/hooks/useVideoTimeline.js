/**
 * Store-backed derivations of the imported-video timeline.
 */

import { useCallback, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { resolveActivityClipId, resolveActivityContext, resolveAvailableMetrics } from '@/lib/activity/activity-context'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { deriveVideoWarnings } from '@/lib/video-program/video-warnings'

/**
 * Returns the committed clips, their timeline program, and covered interval, plus
 * the same values as displayed during a transient clip drag.
 * @returns {{clips: Array<object>, program: object, bounds: object|null, displayed: {clips: Array<object>, program: object, bounds: object|null}}} Memoized timeline.
 */
export function useVideoTimeline() {
  const { importedVideos, videoEdits, videoDragPreview, transitionDefaults } = useStore(
    useShallow((state) => ({
      importedVideos: state.importedVideos,
      videoEdits: state.videoEdits,
      videoDragPreview: state.videoDragPreview,
      transitionDefaults: state.transitionDefaults,
    })),
  )
  const committed = useMemo(
    () => buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults }),
    [importedVideos, transitionDefaults, videoEdits],
  )
  const displayed = useMemo(
    () =>
      videoDragPreview === null
        ? committed
        : buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: videoDragPreview, transitionDefaults }),
    [committed, importedVideos, transitionDefaults, videoDragPreview, videoEdits],
  )
  return useMemo(() => ({ ...committed, displayed }), [committed, displayed])
}

/**
 * Returns the warnings of every committed clip (see `deriveVideoWarnings`).
 * @returns {Map<string, Array<{kind: string, message: string}>>} Warnings keyed by video ID.
 */
export function useVideoWarnings() {
  const { clips, program } = useVideoTimeline()
  return useMemo(() => deriveVideoWarnings(clips, program), [clips, program])
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
 * Returns metrics available for new widgets across the external activity or all embedded telemetry.
 * @returns {Array<{attribute: string, source: string}>} Available metrics.
 */
export function useAvailableMetrics() {
  const { activitySummary, importedVideos } = useStore(
    useShallow((state) => ({ activitySummary: state.activitySummary, importedVideos: state.importedVideos })),
  )
  return useMemo(() => resolveAvailableMetrics(activitySummary, importedVideos), [activitySummary, importedVideos])
}
