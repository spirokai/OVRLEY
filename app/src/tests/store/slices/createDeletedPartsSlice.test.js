import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import { buildProgram } from '@/lib/video-program/program'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '../../helpers/videoFixtures'

const ride = createTestVideo({ id: 'ride', durationSeconds: 60 })
const later = createTestVideo({ id: 'later', path: '/videos/later.mp4', durationSeconds: 30, importOrder: 1 })
const defaults = { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 2 }

function parts(videoId = 'ride') {
  return useStore.getState().videoEdits[videoId].deletedParts
}

function program() {
  const { importedVideos, videoEdits } = useStore.getState()
  return buildProgram({ videos: joinVideoEdits(importedVideos, videoEdits), exportRange: { type: 'all' }, transitionDefaults: defaults })
}

describe('deleted parts store contract', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState(videoStoreState([ride, later], { later: 70 }))
    let nextId = 0
    vi.stubGlobal('crypto', { randomUUID: vi.fn(() => `part-${++nextId}`) })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  test('adds frame-exact parts, merges overlaps, and splits the program at them', () => {
    const state = useStore.getState()
    expect(state.addDeletedPart('ride', 10.01, 20)).toBe('part-1')
    expect(parts()).toEqual([{ id: 'part-1', fromSecond: 10, toSecond: 20, transition: null }])
    expect(state.addDeletedPart('ride', 15, 25)).toBe('part-1')
    expect(parts()).toEqual([{ id: 'part-1', fromSecond: 10, toSecond: 25, transition: null }])

    const { pieces, joins } = program()
    expect(pieces.map((piece) => [piece.videoId, piece.sourceStartSecond, piece.sourceEndSecond])).toEqual([
      ['ride', 0, 10],
      ['ride', 25, 60],
      ['later', 0, 30],
    ])
    // The deleted length is the join's gap, above the continuity threshold.
    expect(joins[0]).toMatchObject({ kind: 'deletedPart', ownerId: 'part-1', gapSeconds: 15, isDefault: true, transition: { type: 'fade' } })
    expect(() => state.addDeletedPart('ride', 30, 30.01)).toThrow(/at least one frame/)
    expect(() => state.addDeletedPart('missing', 1, 2)).toThrow(/Unknown imported video/)
  })

  test('moves and restores parts', () => {
    const state = useStore.getState()
    const first = state.addDeletedPart('ride', 10, 20)
    const second = state.addDeletedPart('ride', 30, 40)
    state.setDeletedPartRange('ride', first, 5, 8)
    expect(parts().map((part) => [part.id, part.fromSecond, part.toSecond])).toEqual([
      [first, 5, 8],
      [second, 30, 40],
    ])
    // Moving onto another part merges both into the earlier one.
    state.setDeletedPartRange('ride', second, 7, 35)
    expect(parts().map((part) => [part.id, part.fromSecond, part.toSecond])).toEqual([[first, 5, 35]])
    state.removeDeletedPart('ride', first)
    expect(parts()).toEqual([])
    expect(() => state.removeDeletedPart('ride', first)).toThrow(/Unknown deleted part/)
  })

  test('overrides join transitions on their owners and returns them to the defaults', () => {
    const state = useStore.getState()
    const partId = state.addDeletedPart('ride', 10, 20)
    const [atPart, intoLater] = program().joins
    state.setJoinTransition(atPart, { type: 'none', durationSeconds: 0 })
    state.setJoinTransition(intoLater, { type: 'wipeleft', durationSeconds: 1.5 })
    expect(parts()[0].transition).toEqual({ type: 'none', durationSeconds: 0 })
    expect(useStore.getState().videoEdits.later.transitionIn).toEqual({ type: 'wipeleft', durationSeconds: 1.5 })
    expect(program().joins.map((join) => [join.transition, join.isDefault])).toEqual([
      [{ type: 'none', durationSeconds: 0 }, false],
      [{ type: 'wipeleft', durationSeconds: 1.5 }, false],
    ])
    state.setJoinTransition({ kind: 'deletedPart', ownerId: partId }, null)
    expect(program().joins[0].isDefault).toBe(true)
    expect(() => state.setJoinTransition(atPart, { type: 'spin', durationSeconds: 1 })).toThrow(/Unknown transition type/)
    expect(() => state.setJoinTransition(atPart, { type: 'fade', durationSeconds: 0 })).toThrow(/positive/)
    expect(() => state.setJoinTransition(atPart, { type: 'none', durationSeconds: 1 })).toThrow(/duration of 0/)
    expect(() => state.setJoinTransition({ kind: 'deletedPart', ownerId: 'missing' }, null)).toThrow(/Unknown deleted part/)
  })

  test('deletes between a marked start and an end in either order', () => {
    const state = useStore.getState()
    state.markDeletedPartStart('ride', 40)
    expect(useStore.getState().deletedPartMark).toEqual({ videoId: 'ride', sourceSecond: 40 })
    const id = state.completeDeletedPart(30)
    expect(parts()).toEqual([{ id, fromSecond: 30, toSecond: 40, transition: null }])
    expect(useStore.getState().deletedPartMark).toBeNull()
    expect(() => state.completeDeletedPart(10)).toThrow(/marked first/)
    expect(() => state.markDeletedPartStart('ride', 61)).toThrow(/inside its video/)
    state.markDeletedPartStart('ride', 5)
    state.removeImportedVideo('ride')
    expect(useStore.getState().deletedPartMark).toBeNull()
  })

  test('deleted part edits are undoable', () => {
    useStore.temporal.getState().clear()
    useStore.getState().addDeletedPart('ride', 10, 20)
    expect(parts()).toHaveLength(1)
    useStore.temporal.getState().undo()
    expect(parts()).toEqual([])
  })
})
