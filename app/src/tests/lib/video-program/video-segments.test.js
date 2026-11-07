import { describe, expect, test } from 'vite-plus/test'
import { cutVideo, removeCut, segmentBeforeCut, setSegmentExcluded, setSegmentTransition } from '@/lib/video-program/collection-edits'
import { findSegmentAt, isVideoExcluded, listVideoSegments, resolveCutFrame, resolveEditClip } from '@/lib/video-program/video-segments'
import { createTestVideo, cutEdits, videoStoreState } from '@/tests/helpers/videoFixtures'

function collection(durationSeconds = 10) {
  return videoStoreState([createTestVideo({ id: 'ride', durationSeconds })])
}

function clip(cuts = [], start = {}) {
  return { ...createTestVideo({ id: 'ride', durationSeconds: 10 }), offsetSeconds: 0, ...cutEdits(cuts, start) }
}

describe('segments of a video', () => {
  test('a video without cuts is one segment', () => {
    expect(listVideoSegments(clip())).toEqual([{ key: 'start', startFrame: 0, endFrame: 300, excluded: false, transitionIn: null }])
    expect(isVideoExcluded(clip())).toBe(false)
    expect(isVideoExcluded(clip([], { excluded: true }))).toBe(true)
  })

  test('cuts split a video into segments keyed by the cut they start at', () => {
    const video = clip([
      { id: 'c1', frame: 100, excluded: true },
      { id: 'c2', frame: 200 },
    ])
    expect(listVideoSegments(video).map(({ key, startFrame, endFrame, excluded }) => [key, startFrame, endFrame, excluded])).toEqual([
      ['start', 0, 100, false],
      ['c1', 100, 200, true],
      ['c2', 200, 300, false],
    ])
    expect(isVideoExcluded(video)).toBe(false)
  })

  test('finds the segment showing at a second, the last one at the very end', () => {
    const video = clip([{ id: 'c1', frame: 150 }])
    expect(findSegmentAt(video, 4.9).key).toBe('start')
    expect(findSegmentAt(video, 5).key).toBe('c1')
    expect(findSegmentAt(video, 10).key).toBe('c1')
  })

  test('a cut goes on a frame strictly inside the video and not on another cut', () => {
    const video = clip([{ id: 'c1', frame: 150 }])
    expect(resolveCutFrame(video, 2)).toBe(60)
    expect(resolveCutFrame(video, 0)).toBeNull()
    expect(resolveCutFrame(video, 10)).toBeNull()
    expect(resolveCutFrame(video, 5)).toBeNull()
  })

  test('commands apply to the selected clip, else the previewed one, else the earliest covering the second', () => {
    const a = { id: 'a', offsetSeconds: 0, durationSeconds: 20, importOrder: 0 }
    const b = { id: 'b', offsetSeconds: 10, durationSeconds: 20, importOrder: 1 }
    expect(resolveEditClip([a, b], 'a', 'b', 15)).toBe(b)
    expect(resolveEditClip([a, b], 'b', null, 15)).toBe(b)
    expect(resolveEditClip([a, b], null, null, 15)).toBe(a)
    // A selected clip that does not cover the second gives way.
    expect(resolveEditClip([a, b], 'a', 'b', 5)).toBe(a)
    expect(resolveEditClip([a, b], null, null, 40)).toBeNull()
  })
})

describe('cut edits', () => {
  test('a new segment keeps the state of the segment it comes from and follows the defaults', () => {
    const draft = collection()
    const fade = { type: 'fade', durationSeconds: 1 }
    draft.videoEdits.ride.segments.start = { excluded: true, transitionIn: fade }
    const later = cutVideo(draft, 'ride', 200)
    const earlier = cutVideo(draft, 'ride', 100)
    expect(draft.videoEdits.ride.cuts).toEqual([
      { id: earlier, frame: 100 },
      { id: later, frame: 200 },
    ])
    expect(draft.videoEdits.ride.segments).toEqual({
      start: { excluded: true, transitionIn: fade },
      [earlier]: { excluded: true, transitionIn: null },
      [later]: { excluded: true, transitionIn: null },
    })
  })

  test.each([[0], [300], [301], [-1], [1.5]])('rejects a cut at frame %s', (frame) => {
    expect(() => cutVideo(collection(), 'ride', frame)).toThrow(/must lie inside/)
  })

  test('rejects a second cut on the same frame and an unknown video', () => {
    const draft = collection()
    cutVideo(draft, 'ride', 100)
    expect(() => cutVideo(draft, 'ride', 100)).toThrow(/already cut/)
    expect(() => cutVideo(draft, 'missing', 100)).toThrow(/Unknown imported video/)
  })

  test('removing a cut merges into the earlier segment, which keeps its transition', () => {
    const draft = collection()
    const fade = { type: 'fade', durationSeconds: 1 }
    const first = cutVideo(draft, 'ride', 100)
    const second = cutVideo(draft, 'ride', 200)
    setSegmentTransition(draft, 'ride', first, fade)
    setSegmentExcluded(draft, 'ride', second, true)
    expect(segmentBeforeCut(draft.videoEdits.ride, first)).toBe('start')
    expect(segmentBeforeCut(draft.videoEdits.ride, second)).toBe(first)

    removeCut(draft, 'ride', second, true)
    expect(draft.videoEdits.ride.cuts).toEqual([{ id: first, frame: 100 }])
    expect(draft.videoEdits.ride.segments[first]).toEqual({ excluded: true, transitionIn: fade })
    expect(draft.videoEdits.ride.segments[second]).toBeUndefined()
    expect(() => removeCut(draft, 'ride', 'missing', false)).toThrow(/Unknown cut/)
  })

  test('segment edits require a known segment and a valid transition', () => {
    const draft = collection()
    expect(() => setSegmentExcluded(draft, 'ride', 'missing', true)).toThrow(/Unknown video segment/)
    expect(() => setSegmentTransition(draft, 'ride', 'start', { type: 'spin', durationSeconds: 1 })).toThrow(/Unknown transition type/)
    expect(() => setSegmentTransition(draft, 'ride', 'start', { type: 'none', durationSeconds: 1 })).toThrow(/duration of 0/)
    expect(() => setSegmentTransition(draft, 'ride', 'start', { type: 'fade', durationSeconds: 0 })).toThrow(/positive/)
    setSegmentTransition(draft, 'ride', 'start', { type: 'fade', durationSeconds: 1 })
    setSegmentTransition(draft, 'ride', 'start', null)
    expect(draft.videoEdits.ride.segments.start.transitionIn).toBeNull()
  })
})
