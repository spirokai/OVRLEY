/**
 * Container hook for OverlayPlayer.
 * Composes store selection, playback engine state, and keyboard shortcuts.
 */

import usePlaybackEngine from './usePlaybackEngine'
import usePlayerKeyboard from './usePlayerKeyboard'
import usePlayerStore from './usePlayerStore'

/**
 * Builds render state and event handlers for the overlay player component.
 *
 * @param {object} props - Hook props.
 * @param {string} props.backgroundMode - Selected canvas background style.
 * @returns {object} State and handlers for rendering OverlayPlayer.
 */
export default function useOverlayPlayerState({ backgroundMode }) {
  // Store selectors
  const playerStore = usePlayerStore()

  // Playback engine - owns timeline playback, source switching, and scrub handlers
  const playback = usePlaybackEngine({
    ...playerStore,
    backgroundMode,
  })

  // Keyboard shortcuts - binds global player shortcuts to playback engine handlers
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
