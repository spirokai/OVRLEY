/**
 * Container hook for OverlayPlayer.
 * Composes store selection, playback engine state, and keyboard shortcuts.
 */

import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import usePlaybackEngine from './usePlaybackEngine'
import usePlayerKeyboard from './usePlayerKeyboard'

export default function useOverlayPlayerState({ backgroundMode }) {
  const playerStore = useStore(
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
      setSelectedSecond: state.setSelectedSecond,
      startPreviewPlayback: state.startPreviewPlayback,
      updatePreviewScrub: state.updatePreviewScrub,
      updateRate: state.updateRate,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
    })),
  )

  const playback = usePlaybackEngine({
    ...playerStore,
    backgroundMode,
  })

  usePlayerKeyboard({
    clampedPlayhead: playback.clampedPlayhead,
    handlePause: playback.handlePause,
    handlePlay: playback.handlePlay,
    handleStep: playback.handleStep,
    hasActivity: playback.hasActivity,
    isPlaying: playback.isPlaying,
    totalDuration: playback.totalDuration,
  })

  return playback
}
