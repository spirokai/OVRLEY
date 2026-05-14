/**
 * Zustand selector hook for the overlay player feature.
 */

import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'

/**
 * Selects store state and actions required by the overlay player.
 *
 * @returns {object} Player store values and action callbacks.
 */
export default function usePlayerStore() {
  return useStore(
    useShallow((state) => ({
      activitySummary: state.activitySummary,
      beginPreviewScrub: state.beginPreviewScrub,
      commitPreviewScrub: state.commitPreviewScrub,
      dummyDurationSeconds: state.dummyDurationSeconds,
      importedVideoDuration: state.importedVideoDuration,
      importedVideoPath: state.importedVideoPath,
      pausePreviewPlayback: state.pausePreviewPlayback,
      previewPlaybackSource: state.previewPlaybackSource,
      previewPlaybackState: state.previewPlaybackState,
      sceneFps: state.config?.scene?.fps ?? 30,
      selectedSecond: state.selectedSecond,
      setSelectedSecondTransient: state.setSelectedSecondTransient,
      startPreviewPlayback: state.startPreviewPlayback,
      updatePreviewScrub: state.updatePreviewScrub,
      updateRate: state.updateRate,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
    })),
  )
}
