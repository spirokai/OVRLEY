/**
 * Edits the project transition defaults from the videos panel.
 */

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { parseSecondsInput, transitionLabel } from '@/features/transitions'
import { NO_TRANSITION } from '@/lib/video-program/transition-catalog'
import useStore from '@/store/useStore'

/**
 * Returns the defaults with commands that store a changed value at once. Joins
 * still on their default follow the change; overridden joins keep their choice.
 * @returns {object} Presentational section model.
 */
export function useTransitionDefaultsSection() {
  const { t } = useTranslation()
  const transitionDefaults = useStore((state) => state.transitionDefaults)
  const setTransitionDefaults = useStore((state) => state.setTransitionDefaults)
  const [error, setError] = useState(null)
  const { type, durationSeconds, splitGapThresholdSeconds } = transitionDefaults
  const commit = (changes) => {
    try {
      setTransitionDefaults({ type, durationSeconds, splitGapThresholdSeconds, ...changes })
      setError(null)
    } catch (failure) {
      setError(failure.message)
    }
  }
  return {
    type,
    typeLabel: type === NO_TRANSITION ? transitionLabel({ type, durationSeconds: 0 }, t) : type,
    previewType: type === NO_TRANSITION ? null : type,
    durationInput: String(durationSeconds),
    thresholdInput: String(splitGapThresholdSeconds),
    error,
    onTypeChange: (value) => commit({ type: value }),
    onDurationCommit: (text) => commit({ durationSeconds: parseSecondsInput(text) }),
    onThresholdCommit: (text) => commit({ splitGapThresholdSeconds: parseSecondsInput(text) }),
  }
}
