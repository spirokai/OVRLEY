import { describe, expect, test } from 'vite-plus/test'

import { fpsEquals, framesToSeconds, secondsToFrames } from '@/lib/video-program/frame-grid'

const NTSC = { num: 30000, den: 1001 }

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
