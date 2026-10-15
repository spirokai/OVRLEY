/**
 * Container hook for the session-level video sync controls.
 *
 * Manages the local shared-offset input state and exposes handlers that coerce,
 * validate, and commit sync values to the global store. Session controls apply to
 * every imported video.
 *
 * @module useVideoSyncControls
 */

import { useEffect, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'

function formatOffsetInput(seconds) {
  const rounded = Math.round(seconds * 10) / 10
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1)
}

/**
 * Provides state and callbacks for the video sync offset controls.
 *
 * The shared offset shows where the earliest video starts; editing it shifts
 * every video by the same amount and keeps per-video adjustments.
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

  const [offsetInput, setOffsetInput] = useState(formatOffsetInput(sharedOffsetSeconds))
  const [inputError, setInputError] = useState(null)

  useEffect(() => {
    setOffsetInput(formatOffsetInput(sharedOffsetSeconds))
  }, [sharedOffsetSeconds])

  const filenameCreationTimeAvailable = useMemo(
    () => importedVideos.some((video) => parseVideoFilenameCreationTime(video.path) !== null),
    [importedVideos],
  )
  const videoSyncWarning = inputError ?? orderedVideos.find((video) => video.syncWarning !== null)?.syncWarning ?? null

  const commitSharedOffset = (seconds) => {
    try {
      shiftVideoOffsets(seconds - sharedOffsetSeconds)
    } catch (error) {
      setInputError(error.message)
      setOffsetInput(formatOffsetInput(sharedOffsetSeconds))
      return false
    }
    setInputError(null)
    setOffsetInput(formatOffsetInput(seconds))
    return true
  }

  const submitOffsetInput = (val) => {
    commitSharedOffset(Math.round(timeToSeconds(val) * 10) / 10)
  }

  const incrementOffset = (amount) => {
    commitSharedOffset(Math.round((timeToSeconds(offsetInput) + amount) * 10) / 10)
  }

  return {
    activitySummary,
    canResetCreationTime: importedVideos.some((video) => video.timeSource === 'filename'),
    filenameCreationTimeAvailable,
    offsetInput,
    timezone,
    timezoneDisabled: importedVideos.length > 0 && importedVideos.every((video) => video.timeSource === 'filename'),
    videoSyncTimezoneMode,
    videoSyncWarning,
    computeVideoSync,
    incrementOffset,
    openManualVideoSync,
    resetVideoCreationTimes,
    setOffsetInput,
    setVideoCreationTimesFromFilenames,
    setVideoSyncTimezoneMode,
    submitOffsetInput,
  }
}
