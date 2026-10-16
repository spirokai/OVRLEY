/**
 * Deleted parts of imported videos and the transitions of program joins.
 *
 * Parts live in `videoEdits[videoId].deletedParts` (undoable, saved with the
 * project); see `lib/video-program/deleted-parts`. A join's transition is stored
 * on its owner: `videoEdits[videoId].transitionIn` for a join into a clip, the
 * deleted part's `transition` for a join at a deleted part. `null` follows the
 * project transition defaults.
 */

import { mergeDeletedPart, snapDeletedRange } from '@/lib/video-program/deleted-parts'
import { NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'

function requireVideo(state, videoId) {
  const video = state.importedVideos.find((candidate) => candidate.id === videoId)
  if (video === undefined) throw new Error(`Unknown imported video: ${videoId}`)
  return video
}

function requireDeletedPart(state, videoId, partId) {
  const part = state.videoEdits[videoId].deletedParts.find((candidate) => candidate.id === partId)
  if (part === undefined) throw new Error(`Unknown deleted part: ${partId}`)
  return part
}

function requireTransition(transition) {
  if (transition === null) return null
  const { type, durationSeconds } = transition
  if (type === NO_TRANSITION) {
    if (durationSeconds !== 0) throw new Error('No transition has a duration of 0 seconds')
  } else if (!XFADE_TRANSITIONS.includes(type)) {
    throw new Error(`Unknown transition type: ${type}`)
  } else if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) {
    throw new Error('Transition duration must be a positive number of seconds')
  }
  return { type, durationSeconds }
}

export const createDeletedPartsSlice = (set, get) => ({
  // Transient start of a deleted part being marked at the playhead: { videoId, sourceSecond } or null.
  deletedPartMark: null,

  /**
   * Deletes a clip-local source range of a video, snapped to its frames and
   * merged with every deleted part it overlaps or touches.
   * @returns {string} ID of the deleted part holding the range.
   */
  addDeletedPart: (videoId, fromSecond, toSecond) => {
    const state = get()
    const range = snapDeletedRange(requireVideo(state, videoId), fromSecond, toSecond)
    const parts = mergeDeletedPart(state.videoEdits[videoId].deletedParts, { id: globalThis.crypto.randomUUID(), ...range, transition: null })
    set((draft) => {
      draft.videoEdits[videoId].deletedParts = parts
    })
    return parts.find((part) => part.fromSecond <= range.fromSecond && part.toSecond >= range.toSecond).id
  },

  /** Moves a deleted part to a new clip-local range, merging it with parts it then overlaps or touches. */
  setDeletedPartRange: (videoId, partId, fromSecond, toSecond) => {
    const state = get()
    const range = snapDeletedRange(requireVideo(state, videoId), fromSecond, toSecond)
    const part = requireDeletedPart(state, videoId, partId)
    const others = state.videoEdits[videoId].deletedParts.filter((candidate) => candidate.id !== partId)
    set((draft) => {
      draft.videoEdits[videoId].deletedParts = mergeDeletedPart(others, { ...part, ...range })
    })
  },

  /** Restores a deleted part to the output. */
  removeDeletedPart: (videoId, partId) => {
    const state = get()
    requireVideo(state, videoId)
    requireDeletedPart(state, videoId, partId)
    set((draft) => {
      draft.videoEdits[videoId].deletedParts = draft.videoEdits[videoId].deletedParts.filter((part) => part.id !== partId)
    })
  },

  /**
   * Overrides the transition of a program join, or with `null` returns it to the defaults.
   * @param {{kind: 'clip'|'deletedPart', ownerId: string}} join Join from `buildProgram`.
   * @param {{type: string, durationSeconds: number}|null} transition Override.
   */
  setJoinTransition: ({ kind, ownerId }, transition) => {
    const state = get()
    const value = requireTransition(transition)
    if (kind === 'clip') {
      requireVideo(state, ownerId)
      set((draft) => {
        draft.videoEdits[ownerId].transitionIn = value
      })
      return
    }
    if (kind !== 'deletedPart') throw new Error(`Unknown join kind: ${kind}`)
    const videoId = Object.keys(state.videoEdits).find((id) => state.videoEdits[id].deletedParts.some((part) => part.id === ownerId))
    if (videoId === undefined) throw new Error(`Unknown deleted part: ${ownerId}`)
    set((draft) => {
      draft.videoEdits[videoId].deletedParts.find((part) => part.id === ownerId).transition = value
    })
  },

  /** Remembers where the deleted part being marked starts. */
  markDeletedPartStart: (videoId, sourceSecond) => {
    const video = requireVideo(get(), videoId)
    if (!Number.isFinite(sourceSecond) || sourceSecond < 0 || sourceSecond > video.durationSeconds) {
      throw new Error('A deleted part must start inside its video')
    }
    set({ deletedPartMark: { videoId, sourceSecond } })
  },

  /**
   * Deletes the range between the marked start and a clip-local end on the marked video.
   * @returns {string} ID of the deleted part holding the range.
   */
  completeDeletedPart: (sourceSecond) => {
    const mark = get().deletedPartMark
    if (mark === null) throw new Error('A deleted part start must be marked first')
    const id = get().addDeletedPart(mark.videoId, Math.min(mark.sourceSecond, sourceSecond), Math.max(mark.sourceSecond, sourceSecond))
    set({ deletedPartMark: null })
    return id
  },

  clearDeletedPartMark: () => {
    set({ deletedPartMark: null })
  },
})
