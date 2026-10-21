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

/**
 * Returns the stored override of a join: `transitionIn` of the clip it leads
 * into, or the transition of the deleted part it bridges.
 * @param {{kind: 'clip'|'deletedPart', ownerId: string}} join Program join.
 * @param {Array<object>} clips Timeline clips with their edits.
 * @returns {{type: string, durationSeconds: number}|null} Override, or null for the defaults.
 */
export function joinOverride(join, clips) {
  if (join.kind === 'clip') return clips.find((clip) => clip.id === join.ownerId).transitionIn
  for (const clip of clips) {
    const part = clip.deletedParts.find((candidate) => candidate.id === join.ownerId)
    if (part !== undefined) return part.transition
  }
  throw new Error(`Unknown deleted part: ${join.ownerId}`)
}

/**
 * Builds the transition control of a join. Without a join (a deleted part at a
 * clip edge) the control is unavailable. Durations are stored snapped to the
 * frames of the joined pieces, as the export counts them.
 * @param {object} options Model inputs.
 * @param {object|undefined} options.join Program join.
 * @param {{num: number, den: number}|null} options.fps Frame rate of the joined pieces; null without a join.
 * @param {{type: string, durationSeconds: number}|null} options.override Stored override.
 * @param {{durationSeconds: number}} options.transitionDefaults Project transition defaults.
 * @param {boolean} options.isInvalid Whether the transition does not fit the pieces it joins.
 * @param {function} options.setJoinTransition Store command.
 * @param {function} options.t Translation function.
 * @returns {object} Presentational control model.
 */
export function buildJoinTransitionModel({ join, fps, override, transitionDefaults, isInvalid, setJoinTransition, t }) {
  const isXfade = override !== null && override.type !== NO_TRANSITION
  const effective = join?.transition ?? null
  return {
    isAvailable: join !== undefined,
    value: override === null ? DEFAULT_TRANSITION_OPTION : override.type,
    label: effective === null ? t('transitions.noJoin', 'No join') : transitionLabel(effective, t),
    defaultLabel:
      join !== undefined && join.isDefault
        ? t('transitions.defaultIs', { defaultValue: 'Default ({{transition}})', transition: transitionLabel(join.transition, t) })
        : t('transitions.default', 'Default'),
    previewType: effective !== null && effective.type !== NO_TRANSITION ? effective.type : null,
    durationSeconds: isXfade ? override.durationSeconds : null,
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
    /** Stores a duration typed into the override (already snapped to `durationFps`). */
    commitDuration: (durationSeconds) => setJoinTransition(join, { type: override.type, durationSeconds }),
  }
}
