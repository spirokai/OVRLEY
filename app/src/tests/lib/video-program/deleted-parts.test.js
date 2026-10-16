import { describe, expect, test } from 'vitest'
import { isDeletableRange, mergeDeletedPart, resolveMarkedClip, snapDeletedRange } from '@/lib/video-program/deleted-parts'
import { createTestVideo } from '@/tests/helpers/videoFixtures'

const video = createTestVideo({ durationSeconds: 10, fps: { num: 30000, den: 1001 } })
const part = (id, fromSecond, toSecond, transition = null) => ({ id, fromSecond, toSecond, transition })

describe('snapDeletedRange', () => {
  test('snaps both bounds to the nearest frame of the video', () => {
    expect(snapDeletedRange(video, 1.01, 2.02)).toEqual({ fromSecond: (30 * 1001) / 30000, toSecond: (61 * 1001) / 30000 })
  })

  test('rejects ranges without a frame inside the video', () => {
    expect(() => snapDeletedRange(video, 1, 1.01)).toThrow(/at least one frame/)
    expect(() => snapDeletedRange(video, -1, 2)).toThrow(/at least one frame/)
    expect(() => snapDeletedRange(video, 2, 11)).toThrow(/at least one frame/)
    expect(() => snapDeletedRange(video, Number.NaN, 2)).toThrow(/finite/)
    expect([isDeletableRange(video, 1, 1.01), isDeletableRange(video, 0, video.durationSeconds), isDeletableRange(video, 0, Number.NaN)]).toEqual([
      false,
      true,
      false,
    ])
  })
})

describe('mergeDeletedPart', () => {
  test('keeps separate parts apart and in source order', () => {
    expect(mergeDeletedPart([part('b', 6, 7)], part('a', 1, 2))).toEqual([part('a', 1, 2), part('b', 6, 7)])
  })

  test('absorbs overlapping and touching parts into the earliest one', () => {
    const fade = { type: 'fade', durationSeconds: 1 }
    const parts = [part('first', 1, 2, fade), part('second', 3, 4), part('far', 8, 9)]
    expect(mergeDeletedPart(parts, part('new', 2, 3.5))).toEqual([part('first', 1, 4, fade), part('far', 8, 9)])
    expect(mergeDeletedPart(parts, part('new', 0.5, 9.5))).toEqual([part('first', 0.5, 9.5, fade)])
  })
})

describe('resolveMarkedClip', () => {
  const clips = [
    { id: 'early', offsetSeconds: 0, durationSeconds: 10, importOrder: 1 },
    { id: 'overlapping', offsetSeconds: 5, durationSeconds: 10, importOrder: 0 },
  ]

  test('prefers the selected clip, then the previewed clip, then the earliest covering clip', () => {
    expect(resolveMarkedClip(clips, 'early', 'overlapping', 7).id).toBe('overlapping')
    expect(resolveMarkedClip(clips, 'overlapping', null, 7).id).toBe('overlapping')
    expect(resolveMarkedClip(clips, null, 'missing', 7).id).toBe('early')
    expect(resolveMarkedClip(clips, 'early', 'early', 12).id).toBe('overlapping')
    expect(resolveMarkedClip(clips, null, null, 20)).toBeNull()
  })
})
