import { describe, expect, test } from 'vite-plus/test'

import { fpsEquals, framesToSeconds, secondsToFrames } from '@/lib/video-program/frame-grid'
import { deriveCompatibility, getVideoEndSecond, getVideosTimelineBounds, sortVideosByTimeline } from '@/lib/video-program/video-collection'

const NTSC = { num: 30000, den: 1001 }

function video(id, { offsetSeconds = 0, durationSeconds = 60, importOrder = 0, width = 1920, height = 1080, fps = { num: 30, den: 1 } } = {}) {
  return { id, offsetSeconds, durationSeconds, importOrder, resolution: { width, height }, fps }
}

describe('frame grid', () => {
  test.each([
    [1.001, NTSC, 30],
    [0.0166, { num: 60, den: 1 }, 1],
    [10, { num: 25, den: 1 }, 250],
  ])('secondsToFrames(%s) rounds to the nearest frame', (seconds, fps, frames) => {
    expect(secondsToFrames(seconds, fps)).toBe(frames)
  })

  test('framesToSeconds is exact for rational rates', () => {
    expect(framesToSeconds(30, NTSC)).toBeCloseTo(1.001, 12)
    expect(framesToSeconds(1800, { num: 30, den: 1 })).toBe(60)
  })

  test('fpsEquals compares rationals without floating point', () => {
    expect(fpsEquals(NTSC, { num: 60000, den: 2002 })).toBe(true)
    expect(fpsEquals(NTSC, { num: 30, den: 1 })).toBe(false)
  })
})

describe('video collection', () => {
  test('orders by timeline start, then import order', () => {
    const videos = [
      video('c', { offsetSeconds: 10, importOrder: 0 }),
      video('b', { offsetSeconds: 0, importOrder: 2 }),
      video('a', { importOrder: 1 }),
    ]
    expect(sortVideosByTimeline(videos).map(({ id }) => id)).toEqual(['a', 'b', 'c'])
    expect(videos.map(({ id }) => id)).toEqual(['c', 'b', 'a'])
  })

  test('reports end seconds and covered bounds', () => {
    const videos = [video('a', { offsetSeconds: -5, durationSeconds: 10 }), video('b', { offsetSeconds: 100, durationSeconds: 30 })]
    expect(getVideoEndSecond(videos[0])).toBe(5)
    expect(getVideosTimelineBounds(videos)).toEqual({ startSecond: -5, endSecond: 130 })
    expect(getVideosTimelineBounds([])).toBeNull()
  })

  test.each([
    {
      name: 'all compatible',
      videos: [video('a'), video('b', { importOrder: 1 })],
      incompatible: [],
    },
    {
      name: 'largest group wins',
      videos: [video('a', { width: 3840, height: 2160 }), video('b', { importOrder: 1 }), video('c', { importOrder: 2 })],
      incompatible: ['a'],
    },
    {
      name: 'tie goes to the earliest import',
      videos: [video('a', { importOrder: 1 }), video('b', { importOrder: 0, fps: NTSC })],
      incompatible: ['a'],
    },
    {
      name: 'equal rationals are the same rate',
      videos: [video('a', { fps: NTSC }), video('b', { importOrder: 1, fps: { num: 60000, den: 2002 } })],
      incompatible: [],
    },
    {
      name: 'display-oriented resolution differs',
      videos: [video('a'), video('b', { importOrder: 1, width: 1080, height: 1920 }), video('c', { importOrder: 2 })],
      incompatible: ['b'],
    },
  ])('deriveCompatibility: $name', ({ videos, incompatible }) => {
    expect([...deriveCompatibility(videos).incompatibleIds].sort()).toEqual(incompatible)
  })

  test('deriveCompatibility exposes the reference format and handles an empty collection', () => {
    expect(deriveCompatibility([video('a', { fps: NTSC })]).reference).toEqual({ resolution: { width: 1920, height: 1080 }, fps: NTSC })
    expect(deriveCompatibility([])).toEqual({ reference: null, incompatibleIds: new Set() })
  })
})
