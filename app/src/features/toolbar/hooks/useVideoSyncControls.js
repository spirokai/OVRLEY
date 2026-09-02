/**
 * Container hook for video sync controls.
 *
 * Manages the local offset input state and exposes handlers that coerce,
 * validate, and commit sync values to the global store.
 *
 * @module useVideoSyncControls
 */

import { useEffect, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import useStore from '@/store/useStore'

function formatOffsetInput(seconds) {
  const rounded = Math.round(seconds * 10) / 10
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1)
}

/**
 * Provides state and callbacks for the video sync offset controls.
 *
 * @returns {object} Sync state and handlers for the video drawer.
 */
export function useVideoSyncControls() {
  const {
    activitySummary,
    computeVideoSync,
    importedVideoPath,
    importedVideoTimeSource,
    resetVideoCreationTime,
    setVideoCreationTimeFromFilename,
    setVideoSyncOffset,
    setVideoSyncTimezoneMode,
    setVideoSyncWarning,
    videoSyncOffsetSeconds,
    videoSyncTimezoneMode,
    videoSyncWarning,
  } = useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      computeVideoSync: state.computeVideoSync,
      importedVideoPath: state.importedVideoPath,
      importedVideoTimeSource: state.importedVideoTimeSource,
      resetVideoCreationTime: state.resetVideoCreationTime,
      setVideoCreationTimeFromFilename: state.setVideoCreationTimeFromFilename,
      setVideoSyncOffset: state.setVideoSyncOffset,
      setVideoSyncTimezoneMode: state.setVideoSyncTimezoneMode,
      setVideoSyncWarning: state.setVideoSyncWarning,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
      videoSyncWarning: state.videoSyncWarning,
    })),
  )

  const timezone = useStore((state) => state.parsedActivity?.metadata?.timezone ?? null)

  const [offsetInput, setOffsetInput] = useState(formatOffsetInput(videoSyncOffsetSeconds ?? 0))

  useEffect(() => {
    setOffsetInput(formatOffsetInput(videoSyncOffsetSeconds ?? 0))
  }, [videoSyncOffsetSeconds])

  const filenameCreationTimeAvailable = useMemo(() => parseVideoFilenameCreationTime(importedVideoPath) !== null, [importedVideoPath])

  const submitOffsetInput = (val) => {
    const parsed = timeToSeconds(val)
    const rounded = Math.round(parsed * 10) / 10
    try {
      setVideoSyncOffset(rounded)
    } catch (error) {
      setVideoSyncWarning(error.message)
      setOffsetInput(formatOffsetInput(videoSyncOffsetSeconds ?? 0))
      return
    }
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1))
  }

  const incrementOffset = (amount) => {
    const current = timeToSeconds(offsetInput)
    const newOffset = Math.round((current + amount) * 10) / 10
    try {
      setVideoSyncOffset(newOffset)
    } catch (error) {
      setVideoSyncWarning(error.message)
      return
    }
    setVideoSyncWarning(null)
    setOffsetInput(Number.isInteger(newOffset) ? newOffset.toString() : newOffset.toFixed(1))
  }

  return {
    activitySummary,
    canResetCreationTime: importedVideoTimeSource === 'filename',
    filenameCreationTimeAvailable,
    offsetInput,
    timezone,
    videoSyncTimezoneMode,
    videoSyncWarning,
    computeVideoSync,
    incrementOffset,
    resetVideoCreationTime,
    setOffsetInput,
    setVideoCreationTimeFromFilename,
    setVideoSyncTimezoneMode,
    submitOffsetInput,
  }
}
