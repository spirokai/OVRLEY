/**
 * Regression tests for paused preview-video synchronization.
 *
 * Switching the canvas background from a non-video mode to video mode should
 * immediately seek the preview element to the current playhead, even while
 * playback stays paused.
 */

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useStore from '@/store/useStore'
import { useVideoPreview } from '@/features/video-preview/hooks/useVideoPreview'
import { useVideoPlaybackClock } from '@/features/video-preview/hooks/useVideoPlaybackClock'

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

  test('does not seek or restart the ended video while timeline playback continues', () => {
    useStore.setState({
      importedVideoDuration: 30,
      previewPlaybackSource: 'timeline',
      previewPlaybackState: 'playing',
      selectedSecond: 35,
      videoSyncOffsetSeconds: 5,
    })

    const video = createVideoStub({ currentTime: 30 })
    const videoRef = { current: video }
    renderHook(() => useVideoPreview(videoRef, true))

    expect(video.play).not.toHaveBeenCalled()
    expect(video.currentTime).toBe(30)
  })

  test('pauses playback when the active video rejects the play request', async () => {
    useStore.setState({
      previewPlaybackSource: 'video',
      previewPlaybackState: 'playing',
    })

    const playError = Object.assign(new Error('Autoplay was blocked'), { name: 'NotAllowedError' })
    const video = createVideoStub({
      play: vi.fn(() => Promise.reject(playError)),
    })
    const videoRef = { current: video }

    renderHook(() => useVideoPreview(videoRef, true))

    await waitFor(() => {
      expect(useStore.getState().previewPlaybackState).toBe('paused')
    })

    expect(useStore.getState().previewPlaybackSource).toBe('video')
    expect(useStore.getState().selectedSecond).toBe(8)
  })

  test('seeks across original parts using the common group offset', async () => {
    useStore.setState({
      importedVideoParts: [
        { path: 'C:/one.mp4', duration: 10, startSeconds: 0, importId: 'one', previewUrl: 'http://localhost/one' },
        { path: 'C:/two.mp4', duration: 20, startSeconds: 10, importId: 'two', previewUrl: 'http://localhost/two' },
      ],
    })
    const video = createVideoStub()
    const { result } = renderHook(() => useVideoPreview({ current: video }, true))
    expect(result.current.importId).toBe('one')
    expect(video.currentTime).toBe(3)
    act(() => useStore.setState({ selectedSecond: 17 }))
    expect(result.current.videoSrc).toBe('http://localhost/two')
    expect(video.currentTime).toBe(2)
    act(() => useStore.setState({ selectedSecond: 6 }))
    expect(result.current.importId).toBe('one')
    expect(video.currentTime).toBe(1)
    act(() => useStore.setState({ selectedSecond: 17, videoSyncOffsetPreviewSeconds: 4 }))
    expect(result.current.importId).toBe('two')
    await waitFor(() => expect(video.currentTime).toBe(3))
    expect(useStore.getState().videoSyncOffsetSeconds).toBe(5)
  })

  test('continues into the next part and hands off only at the end of the group', () => {
    useStore.setState({
      importedVideoParts: [
        { path: 'C:/one.mp4', duration: 10, startSeconds: 0, importId: 'one', previewUrl: 'http://localhost/one' },
        { path: 'C:/two.mp4', duration: 20, startSeconds: 10, importId: 'two', previewUrl: 'http://localhost/two' },
      ],
      previewPlaybackSource: 'video',
      previewPlaybackState: 'playing',
    })
    const videoRef = { current: createVideoStub() }
    const { result } = renderHook(() => useVideoPreview(videoRef, true))
    act(() => useVideoPlaybackClock.mock.lastCall[0].onPlaybackEnded())
    expect(useStore.getState().selectedSecond).toBe(15)
    expect(useStore.getState().previewPlaybackSource).toBe('video')
    expect(result.current.importId).toBe('two')
    act(() => useVideoPlaybackClock.mock.lastCall[0].onPlaybackEnded())
    expect(useStore.getState().selectedSecond).toBe(35)
    expect(useStore.getState().previewPlaybackSource).toBe('timeline')
  })
})
