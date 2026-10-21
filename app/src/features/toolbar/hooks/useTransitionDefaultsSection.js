/**
 * Edits the project transition defaults from the videos panel.
 */

import { useTranslation } from 'react-i18next'
import { transitionLabel } from '@/features/transitions'
import { NO_TRANSITION } from '@/lib/video-program/transition-catalog'
import useStore from '@/store/useStore'

/**
 * Returns the defaults with commands that store a changed value at once. Joins
 * still on their default follow the change; overridden joins keep their choice.
 * The default duration snaps to the render frame rate; each join snaps it again
 * to the frames of the videos it joins.
 * @returns {object} Presentational section model.
 */
export function useTransitionDefaultsSection() {
  const { t } = useTranslation()
  const transitionDefaults = useStore((state) => state.transitionDefaults)
  const renderFps = useStore((state) => state.renderSettings.fps)
  const setTransitionDefaults = useStore((state) => state.setTransitionDefaults)
  const { type, durationSeconds, splitGapThresholdSeconds } = transitionDefaults
  const commit = (changes) => setTransitionDefaults({ type, durationSeconds, splitGapThresholdSeconds, ...changes })
  return {
    type,
    typeLabel: type === NO_TRANSITION ? transitionLabel({ type, durationSeconds: 0 }, t) : type,
    previewType: type === NO_TRANSITION ? null : type,
    durationSeconds,
    durationFps: { num: renderFps, den: 1 },
    thresholdSeconds: splitGapThresholdSeconds,
    onTypeChange: (value) => commit({ type: value }),
    onDurationCommit: (seconds) => commit({ durationSeconds: seconds }),
    onThresholdCommit: (seconds) => commit({ splitGapThresholdSeconds: seconds }),
  }
}
