/**
 * View models of a clip's deleted parts in the videos panel: bounds, the
 * transition of the join each part creates, and removal.
 */

import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import { isDeletableRange } from '@/lib/video-program/deleted-parts'
import { HARD_CUT, NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'

export const DEFAULT_TRANSITION_OPTION = 'default'

/**
 * Formats a clip-local second as `M:SS.cc` for editing.
 * @param {number} seconds Source second.
 * @returns {string} Editable time.
 */
export function formatSourceTimeInput(seconds) {
  const centiseconds = Math.round(seconds * 100)
  const minutes = Math.floor(centiseconds / 6000)
  return `${minutes}:${((centiseconds - minutes * 6000) / 100).toFixed(2).padStart(5, '0')}`
}

function parseDurationInput(text) {
  const trimmed = text.trim()
  return trimmed === '' ? Number.NaN : Number(trimmed)
}

function transitionLabel(transition, t) {
  return transition.type === NO_TRANSITION
    ? t('video-preview.noTransition', 'No transition')
    : t('toolbar.transitionWithDuration', { defaultValue: '{{type}}, {{seconds}} s', type: transition.type, seconds: transition.durationSeconds })
}

function transitionControl(part, join, transitionDefaults, setJoinTransition, t) {
  const override = part.transition
  const isXfade = override !== null && override.type !== NO_TRANSITION
  return {
    // A part at a clip edge (or overlapped away) separates no pieces and has no join.
    isAvailable: join !== undefined,
    value: override === null ? DEFAULT_TRANSITION_OPTION : override.type,
    options: [
      {
        value: DEFAULT_TRANSITION_OPTION,
        label:
          join === undefined || !join.isDefault
            ? t('toolbar.defaultTransition', 'Default')
            : t('toolbar.defaultTransitionIs', { defaultValue: 'Default ({{transition}})', transition: transitionLabel(join.transition, t) }),
      },
      { value: NO_TRANSITION, label: t('video-preview.noTransition', 'No transition') },
      ...XFADE_TRANSITIONS.map((type) => ({ value: type, label: type })),
    ],
    durationInput: isXfade ? String(override.durationSeconds) : null,
    onTypeChange: (value) => {
      if (value === DEFAULT_TRANSITION_OPTION) setJoinTransition(join, null)
      else if (value === NO_TRANSITION) setJoinTransition(join, HARD_CUT)
      else setJoinTransition(join, { type: value, durationSeconds: isXfade ? override.durationSeconds : transitionDefaults.durationSeconds })
    },
    onDurationCommit: (text) => {
      const durationSeconds = parseDurationInput(text)
      if (isXfade && Number.isFinite(durationSeconds) && durationSeconds > 0) setJoinTransition(join, { type: override.type, durationSeconds })
    },
  }
}

/**
 * Builds the rows of a clip's deleted parts in source order. Bound edits that
 * would leave no frame inside the clip are ignored.
 * @param {object} options Row inputs.
 * @param {object} options.clip Timeline clip (video joined with its edits).
 * @param {{joins: Array<object>}} options.program Timeline program.
 * @param {{durationSeconds: number}} options.transitionDefaults Project transition defaults.
 * @param {{setDeletedPartRange: function, removeDeletedPart: function, setJoinTransition: function, seek: function}} options.commands Store commands and playhead seek.
 * @param {function} options.t Translation function.
 * @returns {Array<object>} Presentational rows.
 */
export function buildDeletedPartRows({ clip, program, transitionDefaults, commands, t }) {
  return [...clip.deletedParts]
    .sort((left, right) => left.fromSecond - right.fromSecond)
    .map((part) => {
      const commitRange = (fromSecond, toSecond) => {
        if (isDeletableRange(clip, fromSecond, toSecond)) commands.setDeletedPartRange(clip.id, part.id, fromSecond, toSecond)
      }
      return {
        id: part.id,
        fromInput: formatSourceTimeInput(part.fromSecond),
        toInput: formatSourceTimeInput(part.toSecond),
        durationLabel: formatSourceTimeInput(part.toSecond - part.fromSecond),
        onFromCommit: (text) => commitRange(timeToSeconds(text), part.toSecond),
        onToCommit: (text) => commitRange(part.fromSecond, timeToSeconds(text)),
        onRemove: () => commands.removeDeletedPart(clip.id, part.id),
        onSeek: () => commands.seek(clip.offsetSeconds + part.fromSecond),
        transition: transitionControl(
          part,
          program.joins.find((join) => join.key === `deletedPart:${part.id}`),
          transitionDefaults,
          commands.setJoinTransition,
          t,
        ),
      }
    })
}
