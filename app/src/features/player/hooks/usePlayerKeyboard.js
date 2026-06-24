/**
 * Keyboard shortcut hook for the overlay player.
 */

import { useEffect } from 'react'
import { isInteractiveElement } from '@/lib/utils'

/**
 * Registers global keyboard shortcuts for player playback and timeline stepping.
 *
 * @param {object} options - Keyboard shortcut inputs.
 * @param {number} options.clampedPlayhead - Current playhead constrained to the timeline duration.
 * @param {function} options.handlePause - Callback that pauses playback at the current playhead.
 * @param {function} options.handlePlay - Callback that starts playback from the current playhead.
 * @param {function} options.handleStepByDirection - Callback that steps playback by one second.
 * @param {boolean} options.hasActivity - Whether there is an activity timeline to control.
 * @param {boolean} options.isPlaying - Whether preview playback is currently active.
 * @param {number} options.totalDuration - Total timeline duration in seconds.
 * @returns {void}
 */
export default function usePlayerKeyboard({
  clampedPlayhead,
  handlePause,
  handlePlay,
  handleStepByDirection,
  hasActivity,
  isPlaying,
  totalDuration,
}) {
  // Global shortcuts - maps Space and Arrow keys to playback controls while focus is outside form controls
  useEffect(() => {
    const handleKeyDown = (event) => {
      if (event.repeat || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || !hasActivity) {
        return
      }

      if (event.code === 'ArrowLeft' || event.code === 'ArrowRight') {
        if (isInteractiveElement(event.target)) return
        event.preventDefault()
        const direction = event.code === 'ArrowRight' ? 1 : -1
        handleStepByDirection(direction)
        return
      }

      if (event.code !== 'Space' || !hasActivity) {
        return
      }

      if (event.target instanceof HTMLElement && event.target.closest('input, textarea, select, button, a, [contenteditable="true"]')) {
        return
      }

      event.preventDefault()

      if (isPlaying) {
        handlePause()
        return
      }

      handlePlay()
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [clampedPlayhead, handlePause, handlePlay, handleStepByDirection, hasActivity, isPlaying, totalDuration])
}
