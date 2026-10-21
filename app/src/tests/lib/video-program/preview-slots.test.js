import { describe, expect, test } from 'vitest'
import { assignPreviewSlots, EMPTY_PREVIEW_SLOTS } from '@/lib/video-program/preview-slots'

const piece = (src, pieceId) => ({ src, pieceId })
const a1 = piece('a', 'a:0')
const a2 = piece('a', 'a:600')
const b1 = piece('b', 'b:0')
const c1 = piece('c', 'c:0')

describe('assignPreviewSlots', () => {
  test('starts in the first element and parks the next piece in the second', () => {
    expect(assignPreviewSlots(EMPTY_PREVIEW_SLOTS, { active: a1, preload: b1 })).toEqual({ slots: [a1, b1], activeIndex: 0 })
    expect(assignPreviewSlots(EMPTY_PREVIEW_SLOTS, { active: a1, preload: null })).toEqual({ slots: [a1, null], activeIndex: 0 })
  })

  test('a cut to the parked piece swaps elements and parks the following piece in the freed one', () => {
    const playing = { slots: [a1, b1], activeIndex: 0 }
    expect(assignPreviewSlots(playing, { active: b1, preload: c1 })).toEqual({ slots: [c1, b1], activeIndex: 1 })
    // After a deleted part the same video waits in both elements at different pieces.
    expect(assignPreviewSlots({ slots: [a1, a2], activeIndex: 0 }, { active: a2, preload: b1 })).toEqual({ slots: [b1, a2], activeIndex: 1 })
  })

  test('a jump within the shown video keeps its element and only updates the piece', () => {
    expect(assignPreviewSlots({ slots: [a1, b1], activeIndex: 0 }, { active: a2, preload: b1 })).toEqual({ slots: [a2, b1], activeIndex: 0 })
  })

  test('a jump to an unloaded video reloads the active element unless it holds the piece to park', () => {
    expect(assignPreviewSlots({ slots: [a1, b1], activeIndex: 0 }, { active: c1, preload: null })).toEqual({ slots: [c1, b1], activeIndex: 0 })
    // Jumping back before the shown piece: the shown piece is next, so it stays parked and the other element loads.
    expect(assignPreviewSlots({ slots: [b1, c1], activeIndex: 0 }, { active: a1, preload: b1 })).toEqual({ slots: [b1, a1], activeIndex: 1 })
  })

  test('returns the previous assignment when nothing changes and empties both elements without videos', () => {
    const previous = { slots: [a1, b1], activeIndex: 0 }
    expect(assignPreviewSlots(previous, { active: piece('a', 'a:0'), preload: piece('b', 'b:0') })).toBe(previous)
    expect(assignPreviewSlots(previous, { active: null, preload: null })).toBe(EMPTY_PREVIEW_SLOTS)
    expect(assignPreviewSlots(EMPTY_PREVIEW_SLOTS, { active: null, preload: null })).toBe(EMPTY_PREVIEW_SLOTS)
  })
})
