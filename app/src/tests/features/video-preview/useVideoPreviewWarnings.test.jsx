/**
 * Behavior tests for preview warning lifecycle handling.
 *
 * Metadata and slow-seek warnings are user-visible contracts. These specs lock
 * in when warnings appear, when they clear, and how source changes reset the
 * warning state for the next imported video.
 */

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { METADATA_SOFT_WARNING_MS, SLOW_SEEK_WARNING_COUNT, SLOW_SEEK_WARNING_MS } from '@/features/video-preview/data/videoPreviewConstants'
import { useVideoPreviewWarnings } from '@/features/video-preview/hooks/useVideoPreviewWarnings'

function createVideoStub(overrides = {}) {
  const events = new EventTarget()

  return {
    currentTime: 0,
    duration: 30,
    error: null,
    addEventListener: events.addEventListener.bind(events),
    removeEventListener: events.removeEventListener.bind(events),
    dispatchEvent: events.dispatchEvent.bind(events),
    ...overrides,
  }
}

describe('useVideoPreviewWarnings', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  test('shows the metadata loading warning until metadata arrives and resets preview errors on source changes', () => {
    const setImportedVideoPreviewError = vi.fn()
    const video = createVideoStub()
    const { result, rerender } = renderHook(
      ({ videoSrc }) =>
        useVideoPreviewWarnings({
          setImportedVideoPreviewError,
          videoRef: { current: video },
          videoSrc,
        }),
      {
        initialProps: {
          videoSrc: 'preview-a.mp4',
        },
      },
    )

    act(() => {
      vi.advanceTimersByTime(METADATA_SOFT_WARNING_MS)
    })

    expect(result.current.metadataStatusMessage).toBe('Loading video metadata...')

    act(() => {
      video.dispatchEvent(new Event('loadedmetadata'))
    })

    expect(result.current.metadataStatusMessage).toBe('')

    rerender({
      videoSrc: 'preview-b.mp4',
    })

    expect(setImportedVideoPreviewError).toHaveBeenLastCalledWith(null)
    expect(result.current.metadataStatusMessage).toBe('')
    expect(result.current.seekWarning).toBe('')
    expect(result.current.nativeVideoError).toBe('')
  })

  test('shows the slow-seek warning after consecutive slow seeks', () => {
    let nowMs = 0
    const nowSpy = vi.spyOn(performance, 'now')
    nowSpy.mockImplementation(() => nowMs)
    const setImportedVideoPreviewError = vi.fn()

    const video = createVideoStub()
    const { result } = renderHook(() =>
      useVideoPreviewWarnings({
        setImportedVideoPreviewError,
        videoRef: { current: video },
        videoSrc: 'preview.mp4',
      }),
    )

    for (let index = 0; index < SLOW_SEEK_WARNING_COUNT; index += 1) {
      act(() => {
        video.dispatchEvent(new Event('seeking'))
      })

      nowMs += SLOW_SEEK_WARNING_MS

      act(() => {
        video.dispatchEvent(new Event('seeked'))
      })
    }

    expect(result.current.seekWarning).toBe('Seeking is slow for this file. A lower-resolution preview proxy may improve responsiveness.')
  })
})
