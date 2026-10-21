/**
 * Container hook for the session-level video sync controls.
 *
 * Exposes the sync state of the imported videos and the handlers that commit
 * sync values to the global store. Session controls apply to every imported video.
 *
 * @module useVideoSyncControls
 */

import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { deriveVideoSyncControls } from '../utils/videoSyncControls'

/**
 * Provides state and callbacks for the video sync controls of the drawer.
 * @returns {object} Sync state and handlers for the video drawer.
 */
export function useVideoSyncControls() {
  const collection = useStore(
    useShallow((state) => ({
      importedVideos: state.importedVideos,
      videoEdits: state.videoEdits,
      videoSyncTimezoneMode: state.videoSyncTimezoneMode,
      parsedActivity: state.parsedActivity,
      activitySummary: state.activitySummary,
    })),
  )
  const commands = useStore(
    useShallow((state) => ({
      computeVideoSync: state.computeVideoSync,
      openManualVideoSync: state.openManualVideoSync,
      resetVideoCreationTimes: state.resetVideoCreationTimes,
      setVideoCreationTimesFromFilenames: state.setVideoCreationTimesFromFilenames,
      setVideoSyncTimezoneMode: state.setVideoSyncTimezoneMode,
      shiftVideoOffsets: state.shiftVideoOffsets,
    })),
  )
  const controls = useMemo(() => deriveVideoSyncControls(collection), [collection])
  const { shiftVideoOffsets, ...actions } = commands

  /** Shifts every video so the earliest starts at `seconds`; returns the reason when that is rejected. */
  const commitSharedOffset = (seconds) => {
    try {
      shiftVideoOffsets(seconds - controls.sharedOffsetSeconds)
    } catch (error) {
      return error.message
    }
    return null
  }

  return { ...controls, ...actions, commitSharedOffset }
}
