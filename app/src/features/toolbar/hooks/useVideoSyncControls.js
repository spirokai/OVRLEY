/**
 * Container hook for the session-level video sync controls.
 *
 * Exposes the shared offset and the handlers that commit sync values to the
 * global store. Session controls apply to every imported video.
 *
 * @module useVideoSyncControls
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'

/**
 * Provides state and callbacks for the video sync offset controls.
 *
 * The shared offset shows where the earliest video starts, on that video's
 * frames; editing it shifts every video by the same amount and keeps per-video
 * adjustments.
 *
 * @returns {object} Sync state and handlers for the video drawer.
 */
export function useVideoSyncControls() {
  const {
    activitySummary,
    computeVideoSync,
    importedVideos,
    videoEdits,
    openManualVideoSync,
    resetVideoCreationTimes,
    setVideoCreationTimesFromFilenames,
    setVideoSyncTimezoneMode,
    shiftVideoOffsets,
    videoSyncTimezoneMode,
  } = useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      computeVideoSync: state.computeVideoSync,
      importedVideos: state.importedVideos,
      videoEdits: state.videoEdits,
      openManualVideoSync: state.openManualVideoSync,
      resetVideoCreationTimes: state.resetVideoCreationTimes,
      setVideoCreationTimesFromFilenames: state.setVideoCreationTimesFromFilenames,
      setVideoSyncTimezoneMode: state.setVideoSyncTimezoneMode,
      shiftVideoOffsets: state.shiftVideoOffsets,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
    })),
  )

  const timezone = useStore((state) => state.parsedActivity?.metadata?.timezone ?? null)
  const orderedVideos = useMemo(() => sortVideosByTimeline(joinVideoEdits(importedVideos, videoEdits)), [importedVideos, videoEdits])
  const sharedOffsetSeconds = orderedVideos[0]?.offsetSeconds ?? 0
  const sharedOffsetFps = orderedVideos[0]?.fps ?? null

  const filenameCreationTimeAvailable = useMemo(
    () => importedVideos.some((video) => parseVideoFilenameCreationTime(video.path) !== null),
    [importedVideos],
  )
  const videoSyncWarning = orderedVideos.find((video) => video.syncWarning !== null)?.syncWarning ?? null

  /** Shifts every video so the earliest starts at `seconds`; returns the reason when that is rejected. */
  const commitSharedOffset = (seconds) => {
    try {
      shiftVideoOffsets(seconds - sharedOffsetSeconds)
    } catch (error) {
      return error.message
    }
    return null
  }

  return {
    activitySummary,
    canResetCreationTime: importedVideos.some((video) => video.timeSource === 'filename'),
    filenameCreationTimeAvailable,
    sharedOffsetSeconds,
    sharedOffsetFps,
    timezone,
    timezoneDisabled: importedVideos.length > 0 && importedVideos.every((video) => video.timeSource === 'filename'),
    videoSyncTimezoneMode,
    videoSyncWarning,
    computeVideoSync,
    commitSharedOffset,
    openManualVideoSync,
    resetVideoCreationTimes,
    setVideoCreationTimesFromFilenames,
    setVideoSyncTimezoneMode,
  }
}
