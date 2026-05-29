/**
 * Regression tests for preview clock mode selection.
 *
 * The video preview clock still needs a developer override for debugging, but
 * that override must now be session-only. These tests document that legacy
 * browser-storage flags are ignored and the hook falls back to the default
 * `'auto'` mode unless the current session opts in explicitly.
 */

import { renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

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
      importedVideoFps: undefined,
      updateRate: 1,
    }),
  ),
}))

vi.mock('@/features/overlay-editor', () => ({
  getEffectivePreviewFps: vi.fn(() => 30),
}))

vi.mock('@/lib/previewPerf', () => ({
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
        onPreviewSecond: vi.fn(),
        videoSyncOffsetSeconds: 0,
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
        onPreviewSecond: vi.fn(),
        videoSyncOffsetSeconds: 0,
      }),
    )

    expect(setPreviewPerfValueSpy).toHaveBeenCalledWith('preview clock mode', 'raf')
  })
})
