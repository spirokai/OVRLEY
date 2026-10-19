/**
 * The incoming overlay of a transition window in the editor preview.
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { useTransitionProxyJobs, useVideoTimeline } from '@/hooks/useVideoTimeline'
import { hasActivityCoverage, resolveActivityContext } from '@/lib/activity/activity-context'
import { resolveTransitionWindow } from '@/lib/video-program/transition-proxies'
import useStore from '@/store/useStore'
import { resolvePreviewExportRange } from '../utils/exportRange'

/**
 * While the preview shows a transition proxy, the overlay blends from the
 * outgoing piece at the playhead into the incoming piece at its own time
 * (`incoming.timelineStart + elapsed`), as the export does. Without a ready proxy
 * the preview cuts and there is no incoming overlay.
 * @returns {{progress: number, activity: object|null, previewSecond: number, hasActivityCoverage: boolean, exportRange: object}|null}
 *   Incoming overlay inputs with the blend progress, or null outside a shown transition.
 */
export function useTransitionOverlay() {
  const { parsedActivity, activitySummary, selectedSecond, transitionProxies, exportRange, isDragPreview } = useStore(
    useShallow((state) => ({
      parsedActivity: state.parsedActivity,
      activitySummary: state.activitySummary,
      selectedSecond: state.selectedSecond,
      transitionProxies: state.transitionProxies,
      exportRange: state.renderSettings.range,
      isDragPreview: state.videoDragPreview !== null,
    })),
  )
  const { clips, program, bounds } = useVideoTimeline().displayed
  const jobs = useTransitionProxyJobs()
  return useMemo(() => {
    const window = isDragPreview ? null : resolveTransitionWindow(program, selectedSecond)
    const job = window === null ? undefined : jobs.find((candidate) => candidate.joinKey === window.join.key)
    if (job === undefined || transitionProxies[job.key]?.status !== 'ready') return null
    const clip = clips.find((candidate) => candidate.id === window.incomingPiece.videoId)
    const context = resolveActivityContext({ parsedActivity, activitySummary, clip })
    const { originSecond } = context
    return {
      progress: window.progress,
      activity: context.activity,
      previewSecond: window.incomingSecond - originSecond,
      hasActivityCoverage: hasActivityCoverage({ parsedActivity, activitySummary, program, clips, second: window.incomingSecond }),
      exportRange: resolvePreviewExportRange({
        exportRange,
        activityOriginSecond: originSecond,
        activityClipDurationSeconds: context.videoId === null ? null : clip.durationSeconds,
        videoBounds: bounds,
      }),
    }
  }, [activitySummary, bounds, clips, exportRange, isDragPreview, jobs, parsedActivity, program, selectedSecond, transitionProxies])
}
