/**
 * Regression tests for preview clock mode selection.
 *
 * The video preview clock still needs a developer override for debugging, but
 * that override must now be session-only. These tests document that legacy
 * browser-storage flags are ignored and the hook falls back to the default
 * `'auto'` mode unless the current session opts in explicitly.
 */

import { renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'

const { setPreviewPerfValueSpy } = vi.hoisted(() => ({
  setPreviewPerfValueSpy: vi.fn(),
}))

vi.mock('@/store/useStore', () => ({
  default: vi.fn((selector) =>
    selector({
      config: {
        scene: {
          fps: 30,
        },
      },
      renderSettings: { fps: 30, widgetUpdateRate: 1 },
    }),
  ),
}))

vi.mock('@/lib/preview-perf', () => ({
  incrementPreviewPerfCounter: vi.fn(),
  previewPerfCounterName: vi.fn((label) => `${label}/s`),
  setPreviewPerfValue: setPreviewPerfValueSpy,
}))

import { useVideoPlaybackClock } from '@/features/video-preview/hooks/useVideoPlaybackClock'

describe('useVideoPlaybackClock', () => {
  beforeEach(() => {
    localStorage.clear()
    delete window.__OVRLEY_PREVIEW_CLOCK_MODE__
    setPreviewPerfValueSpy.mockReset()
  })

  afterEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
    delete window.__OVRLEY_PREVIEW_CLOCK_MODE__
  })

  test('ignores legacy browser-storage overrides and defaults to auto clock mode', () => {
    localStorage.setItem('ovrley:preview-clock-mode', 'raf')
    const getItemSpy = vi.spyOn(Storage.prototype, 'getItem')

    renderHook(() =>
      useVideoPlaybackClock({
        videoRef: { current: null },
        isActive: false,
        onPlaybackEnded: vi.fn(),
        onPreviewSecond: vi.fn(),
        videoFps: null,
        videoOffsetSeconds: 0,
      }),
    )

    expect(getItemSpy).not.toHaveBeenCalled()
    expect(setPreviewPerfValueSpy).toHaveBeenCalledWith('preview clock mode', 'auto')
  })

  test('supports a session-only raf override without using browser storage', () => {
    window.__OVRLEY_PREVIEW_CLOCK_MODE__ = 'raf'

    renderHook(() =>
      useVideoPlaybackClock({
        videoRef: { current: null },
        isActive: false,
        onPlaybackEnded: vi.fn(),
        onPreviewSecond: vi.fn(),
        videoFps: null,
        videoOffsetSeconds: 0,
      }),
    )

    expect(setPreviewPerfValueSpy).toHaveBeenCalledWith('preview clock mode', 'raf')
  })

  test('hands playback on once when the presented frame leaves the playing piece', () => {
    const listeners = {}
    let frameCallback = null
    const video = {
      currentTime: 4,
      paused: false,
      ended: false,
      currentSrc: 'http://preview/video',
      requestVideoFrameCallback: vi.fn((callback) => {
        frameCallback = callback
        return 1
      }),
      cancelVideoFrameCallback: vi.fn(() => {
        frameCallback = null
      }),
      addEventListener: vi.fn((name, listener) => {
        listeners[name] = listener
      }),
      removeEventListener: vi.fn(),
    }
    const onPreviewSecond = vi.fn()
    const onPlaybackEnded = vi.fn()

    renderHook(() =>
      useVideoPlaybackClock({
        videoRef: { current: video },
        isActive: true,
        videoFps: { num: 30, den: 1 },
        videoOffsetSeconds: 10,
        pieceEndSourceSecond: 5,
        onPreviewSecond,
        onPlaybackEnded,
      }),
    )

    onPreviewSecond.mockClear()
    frameCallback(0, { mediaTime: 4.95 })
    expect(onPreviewSecond).toHaveBeenLastCalledWith(14.95)
    expect(onPlaybackEnded).not.toHaveBeenCalled()

    frameCallback(0, { mediaTime: 5 })
    expect(onPreviewSecond).toHaveBeenCalledTimes(1)
    expect(onPlaybackEnded).toHaveBeenCalledOnce()
    expect(frameCallback).toBeNull()

    listeners.ended()
    expect(onPlaybackEnded).toHaveBeenCalledOnce()
  })
})
