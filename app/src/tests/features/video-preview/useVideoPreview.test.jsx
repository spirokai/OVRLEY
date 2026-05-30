/**
 * Regression tests for paused preview-video synchronization.
 *
 * Switching the canvas background from a non-video mode to video mode should
 * immediately seek the preview element to the current playhead, even while
 * playback stays paused.
 */

import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useStore from '@/store/useStore'
import { useVideoPreview } from '@/features/video-preview/hooks/useVideoPreview'

vi.mock('@/features/video-preview/hooks/useVideoPlaybackClock', () => ({
  useVideoPlaybackClock: vi.fn(),
}))

vi.mock('@/features/video-preview/hooks/useVideoPreviewWarnings', () => ({
  useVideoPreviewWarnings: vi.fn(() => ({
    metadataStatusMessage: '',
    nativeVideoError: '',
    seekWarning: '',
  })),
}))

function createVideoStub(overrides = {}) {
  return {
    currentTime: 0,
    duration: 30,
    paused: true,
    play: vi.fn(() => Promise.resolve()),
    pause: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    ...overrides,
  }
}

describe('useVideoPreview', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      importedVideoDuration: 30,
      importedVideoPath: 'C:\\clips\\ride.mp4',
      importedVideoPreviewUrl: 'http://127.0.0.1:3210/preview/ride.mp4',
      previewPlaybackSource: 'timeline',
      previewPlaybackState: 'paused',
      selectedSecond: 8,
      videoSyncOffsetSeconds: 5,
    })
  })

  test('seeks to the paused playhead as soon as video view becomes active', async () => {
    const video = createVideoStub()
    const videoRef = { current: null }
    const { rerender } = renderHook(({ isActive }) => useVideoPreview(videoRef, isActive), {
      initialProps: {
        isActive: false,
      },
    })

    videoRef.current = video
    rerender({
      isActive: true,
    })

    await waitFor(() => {
      expect(video.currentTime).toBe(3)
    })

    expect(video.play).not.toHaveBeenCalled()
  })
})
