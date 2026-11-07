/**
 * Built-in FFmpeg `xfade` transitions (FFmpeg 8.1, `transition=custom` excluded) and transition
 * defaults, from `assets/transitions.json` (shared with the native backend).
 */

import transitionCatalogue from '../../../../assets/transitions.json'

export const NO_TRANSITION = 'none'

export const XFADE_TRANSITIONS = Object.freeze([...transitionCatalogue.xfade])

/** Hard defaults for new and migrated projects. */
export const DEFAULT_TRANSITION_SETTINGS = Object.freeze({ ...transitionCatalogue.defaults })

export const HARD_CUT = Object.freeze({ type: NO_TRANSITION, durationSeconds: 0 })

/**
 * Validates a transition type.
 * @param {string} type Transition type.
 */
export function requireTransitionType(type) {
  if (type !== NO_TRANSITION && !XFADE_TRANSITIONS.includes(type)) throw new Error(`Unknown transition type: ${type}`)
}

/**
 * Validates a stored transition override.
 * @param {{type: string, durationSeconds: number}|null} transition Override, or null to follow the defaults.
 * @returns {{type: string, durationSeconds: number}|null} The canonical value to store.
 */
export function requireTransition(transition) {
  if (transition === null) return null
  const { type, durationSeconds } = transition
  requireTransitionType(type)
  if (type === NO_TRANSITION) {
    if (durationSeconds !== 0) throw new Error('No transition has a duration of 0 seconds')
  } else if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) {
    throw new Error('Transition duration must be a positive number of seconds')
  }
  return { type, durationSeconds }
}
