import { describe, expect, test } from 'vitest'
import { clampVideoTime, getLastDrawableVideoSecond, primeVideoFirstFrame } from '@/features/video-preview/utils/videoPreviewPlayback'

function createVideoStub(overrides = {}) {
  return {
    currentTime: 0,
    duration: 30,
    readyState: 1,
    ...overrides,
  }
}

describe('videoPreviewPlayback helpers', () => {
  test('keeps exact end-time seeks inside the last drawable frame', () => {
    const video = createVideoStub({ duration: 30 })

    expect(clampVideoTime(video, 30)).toBe(29.999)
    expect(clampVideoTime(video, 31)).toBe(29.999)
  })

  test('uses zero as the last drawable frame for extremely short videos', () => {
    const video = createVideoStub({ duration: 0.0005 })

    expect(getLastDrawableVideoSecond(video)).toBe(0)
    expect(clampVideoTime(video, 0.0005)).toBe(0)
  })

  test('primes a metadata-only video at the beginning so the first frame can paint', () => {
    const video = createVideoStub()

    expect(primeVideoFirstFrame(video)).toBe(true)
    expect(video.currentTime).toBe(0.001)
  })

  test('does not prime when a frame is already available', () => {
    const video = createVideoStub({ readyState: 2 })

    expect(primeVideoFirstFrame(video)).toBe(false)
    expect(video.currentTime).toBe(0)
  })

  test('does not prime after normal playhead sync has already seeked away from zero', () => {
    const video = createVideoStub({ currentTime: 3 })

    expect(primeVideoFirstFrame(video)).toBe(false)
    expect(video.currentTime).toBe(3)
  })
})
