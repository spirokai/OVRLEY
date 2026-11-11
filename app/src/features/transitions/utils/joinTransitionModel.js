/**
 * View model of one join's transition: what it is (an override or the project
 * default), the choices, the override's duration, and validation.
 */

import { snapDurationToFrames } from '@/lib/video-program/frame-grid'
import { HARD_CUT, NO_TRANSITION } from '@/lib/video-program/transition-catalog'

export const DEFAULT_TRANSITION_OPTION = 'default'

/**
 * Labels a transition for display.
 * @param {{type: string, durationSeconds: number}} transition Transition.
 * @param {function} t Translation function.
 * @returns {string} Label.
 */
export function transitionLabel(transition, t) {
  return transition.type === NO_TRANSITION
    ? t('transitions.none', 'No transition')
    : t('transitions.withDuration', { defaultValue: '{{type}}, {{seconds}} s', type: transition.type, seconds: transition.durationSeconds })
}

const SKIPPED_DEFAULT_PARTS = {
  previous: ['transitions.partBefore', 'the part before'],
  next: ['transitions.partAfter', 'the part after'],
}

/**
 * Explains why a join that follows the defaults is a hard cut: the default transition does not fit a piece it joins.
 * @param {{side: 'previous'|'next', durationSeconds: number, roomSeconds: number}} skippedDefault Program join's `skippedDefault`.
 * @param {function} t Translation function.
 * @returns {string} Explanation.
 */
export function skippedDefaultLabel({ side, durationSeconds, roomSeconds }, t) {
  return t('transitions.skippedDefault', {
    defaultValue: 'Default transition skipped: {{part}} this join has room for {{room}} s of transition, not {{seconds}} s.',
    part: t(...SKIPPED_DEFAULT_PARTS[side]),
    room: Math.max(0, Math.floor(roomSeconds * 10) / 10),
    seconds: Number(durationSeconds.toFixed(1)),
  })
}

/**
 * Returns the stored override of a join: `transitionIn` of the segment it leads into.
 * @param {{videoId: string, segmentKey: string}} join Program join.
 * @param {Array<object>} clips Timeline clips with their edits.
 * @returns {{type: string, durationSeconds: number}|null} Override, or null for the defaults.
 */
export function joinOverride(join, clips) {
  return clips.find((clip) => clip.id === join.videoId).segments[join.segmentKey].transitionIn
}

/**
 * Builds the transition control of a join. Durations are stored snapped to the
 * frames of the joined pieces, as the export counts them.
 * @param {object} options Model inputs.
 * @param {object} options.join Program join.
 * @param {{num: number, den: number}} options.fps Frame rate of the joined pieces.
 * @param {{type: string, durationSeconds: number}|null} options.override Stored override.
 * @param {{durationSeconds: number}} options.transitionDefaults Project transition defaults.
 * @param {boolean} options.isInvalid Whether the transition does not fit the pieces it joins.
 * @param {function} options.setJoinTransition Store command.
 * @param {function} options.t Translation function.
 * @returns {object} Presentational control model.
 */
export function buildJoinTransitionModel({ join, fps, override, transitionDefaults, isInvalid, setJoinTransition, t }) {
  const isXfade = override !== null && override.type !== NO_TRANSITION
  const effective = join.transition
  const isCut = effective.type === NO_TRANSITION
  return {
    value: override === null ? DEFAULT_TRANSITION_OPTION : override.type,
    label: transitionLabel(effective, t),
    typeLabel: isCut ? t('transitions.none', 'No transition') : effective.type,
    defaultLabel: join.isDefault
      ? t('transitions.defaultIs', { defaultValue: 'Default ({{transition}})', transition: transitionLabel(join.transition, t) })
      : t('transitions.default', 'Default'),
    previewType: isCut ? null : effective.type,
    durationSeconds: isCut ? null : effective.durationSeconds,
    durationFps: fps,
    error: isInvalid ? t('transitions.tooLong', 'Too long for the video pieces it joins') : null,
    onSelect: (value) => {
      if (value === DEFAULT_TRANSITION_OPTION) setJoinTransition(join, null)
      else if (value === NO_TRANSITION) setJoinTransition(join, HARD_CUT)
      else {
        setJoinTransition(join, {
          type: value,
          durationSeconds: isXfade ? override.durationSeconds : snapDurationToFrames(transitionDefaults.durationSeconds, fps),
        })
      }
    },
    /** Stores a typed duration (already snapped to `durationFps`) as an override of the effective transition. */
    commitDuration: (durationSeconds) => setJoinTransition(join, { type: effective.type, durationSeconds }),
  }
}
