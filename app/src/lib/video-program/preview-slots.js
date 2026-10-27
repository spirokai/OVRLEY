/**
 * Assigns the preview's two video elements: the active element shows the piece at
 * the playhead, the other one waits paused on the next piece so that a cut is a
 * swap instead of a reload.
 */

/** Initial assignment: both elements empty, the first one active. */
export const EMPTY_PREVIEW_SLOTS = Object.freeze({ slots: Object.freeze([null, null]), activeIndex: 0 })

function sameSlot(left, right) {
  return left === right || (left !== null && right !== null && left.src === right.src && left.pieceId === right.pieceId)
}

/**
 * Derives the next assignment from the previous one.
 *
 * The active slot is, in order of preference: the slot already prepared with the
 * active piece, the slot already holding the active source (it seeks), or a slot
 * that is (re)loaded with it — the current active one unless that one holds the
 * piece to preload. The other slot then holds the piece to preload (unchanged
 * when there is none).
 * @param {{slots: Array<{src: string, pieceId: string}|null>, activeIndex: number}} previous Previous assignment.
 * @param {{active: {src: string, pieceId: string}|null, preload: {src: string, pieceId: string}|null}} desired Wanted pieces.
 * @returns {{slots: Array<{src: string, pieceId: string}|null>, activeIndex: number}} Next assignment; `previous` when unchanged.
 */
export function assignPreviewSlots(previous, { active, preload }) {
  if (active === null) return previous.slots.every((slot) => slot === null) ? previous : EMPTY_PREVIEW_SLOTS

  const { slots } = previous
  const other = 1 - previous.activeIndex
  let activeIndex
  if (sameSlot(slots[previous.activeIndex], active)) activeIndex = previous.activeIndex
  else if (sameSlot(slots[other], active)) activeIndex = other
  else if (slots[previous.activeIndex]?.src === active.src) activeIndex = previous.activeIndex
  else if (slots[other]?.src === active.src) activeIndex = other
  else activeIndex = preload !== null && sameSlot(slots[previous.activeIndex], preload) ? other : previous.activeIndex

  const next = [...slots]
  next[activeIndex] = active
  if (preload !== null) next[1 - activeIndex] = preload
  const unchanged = activeIndex === previous.activeIndex && next.every((slot, index) => sameSlot(slot, slots[index]))
  return unchanged ? previous : { slots: next, activeIndex }
}
