/**
 * View models of a clip's deleted parts in the videos panel: bounds, the
 * transition of the join each part creates, and removal.
 */

import { timeToSeconds } from '@/lib/time-format'
import { buildJoinTransitionModel } from '@/features/transitions'
import { isDeletableRange } from '@/lib/video-program/deleted-parts'

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

/**
 * Builds the rows of a clip's deleted parts in source order. Bound edits that
 * would leave no frame inside the clip are ignored. A part at a clip edge joins
 * nothing, so its transition control is unavailable.
 * @param {object} options Row inputs.
 * @param {object} options.clip Timeline clip (video joined with its edits).
 * @param {Map<string, object>} options.joinModels Transition controls keyed by join key (`useJoinTransitionModels`).
 * @param {{setDeletedPartRange: function, removeDeletedPart: function, seek: function}} options.commands Store commands and playhead seek.
 * @param {function} options.t Translation function.
 * @returns {Array<object>} Presentational rows.
 */
export function buildDeletedPartRows({ clip, joinModels, commands, t }) {
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
        transition:
          joinModels.get(`deletedPart:${part.id}`) ??
          buildJoinTransitionModel({
            join: undefined,
            fps: null,
            override: part.transition,
            transitionDefaults: null,
            isInvalid: false,
            setJoinTransition: () => {
              throw new Error('A deleted part without a join has no transition')
            },
            t,
          }),
      }
    })
}
