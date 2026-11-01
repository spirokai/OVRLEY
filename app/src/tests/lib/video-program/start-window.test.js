import { describe, expect, test } from 'vite-plus/test'
import { startsOutsideWindow, VIDEO_START_WINDOW_SECONDS } from '@/lib/video-program/start-window'

describe('startsOutsideWindow', () => {
  test('accepts starts up to the window before the reference start or after its end', () => {
    const reference = { from: 100, to: 500 }
    expect(startsOutsideWindow(100 - VIDEO_START_WINDOW_SECONDS, reference)).toBe(false)
    expect(startsOutsideWindow(500 + VIDEO_START_WINDOW_SECONDS, reference)).toBe(false)
    expect(startsOutsideWindow(300, reference)).toBe(false)
    expect(startsOutsideWindow(100 - VIDEO_START_WINDOW_SECONDS - 0.04, reference)).toBe(true)
    expect(startsOutsideWindow(500 + VIDEO_START_WINDOW_SECONDS + 0.04, reference)).toBe(true)
  })
})
