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
