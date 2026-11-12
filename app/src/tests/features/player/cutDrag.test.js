import { describe, expect, test } from 'vite-plus/test'
import { applyCutDraft, applyCutDraftToSelection, clampCutFrame, listEdgeCuts, segmentOfCut } from '@/features/player/utils/cutDrag'
import { cutEdits } from '@/tests/helpers/videoFixtures'

// 300 frames, cut at frame 150; the part after it is excluded.
const clip = { id: 'ride', frameCount: 300, ...cutEdits([{ id: 'cut', frame: 150, excluded: true }]) }

describe('cut selection', () => {
  test('a pressed cut belongs to the part an edge cut trims into, or to the part before a cut', () => {
    expect(segmentOfCut(clip, 'edge-start')).toBe('start')
    expect(segmentOfCut(clip, 'edge-end')).toBe('cut')
    expect(segmentOfCut(clip, 'cut')).toBe('start')
  })

  test('a selected first segment follows a dragged start edge cut', () => {
    const selection = { laneId: 'video:ride', segmentKey: 'start' }
    expect(applyCutDraftToSelection(selection, { videoId: 'ride', cutId: 'edge-start', frame: 40 })).toEqual({
      laneId: 'video:ride',
      segmentKey: 'edge-start',
    })
    expect(applyCutDraftToSelection(selection, { videoId: 'ride', cutId: 'edge-end', frame: 280 })).toBe(selection)
    expect(applyCutDraftToSelection(selection, null)).toBe(selection)
  })
})

describe('edge cuts', () => {
  test('exist only at an end whose outer part is still exported', () => {
    expect(listEdgeCuts(clip)).toEqual([{ id: 'edge-start', frame: 0 }])
  })

  test('stop before the nearest cut, and dragged back past their edge leave no trim', () => {
    expect(clampCutFrame(clip, 'edge-start', 40)).toBe(40)
    expect(clampCutFrame(clip, 'edge-start', 200)).toBe(149)
    expect(clampCutFrame(clip, 'edge-start', -5)).toBeNull()
    const uncut = { id: 'ride', frameCount: 300, ...cutEdits() }
    expect(clampCutFrame(uncut, 'edge-end', 280)).toBe(280)
    expect(clampCutFrame(uncut, 'edge-end', 300)).toBeNull()
  })

  test('show while dragged as a cut with the outer part excluded, the rest of the part keeping its edits', () => {
    const [shown] = applyCutDraft([clip], { videoId: 'ride', cutId: 'edge-start', frame: 40 })

    expect(shown.cuts).toEqual([
      { id: 'edge-start', frame: 40 },
      { id: 'cut', frame: 150 },
    ])
    expect(shown.segments).toEqual({
      start: { excluded: true, transitionIn: null, skipOverlay: false },
      'edge-start': clip.segments.start,
      cut: clip.segments.cut,
    })
    // While it is dragged, the edge cut's own draft does not limit it.
    expect(clampCutFrame(shown, 'edge-start', 100)).toBe(100)
  })
})
