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
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

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
      ...videoStoreState([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 30 })], { 'video-1': 5 }),
      previewPlaybackSource: 'timeline',
      previewPlaybackState: 'paused',
      selectedSecond: 8,
    })
  })

  test('seeks to the paused playhead as soon as video view becomes active', async () => {
    const video = createVideoStub()
    const videoRef = { current: null }
    const { rerender } = renderHook(({ isActive }) => useVideoPreview([videoRef, { current: null }], isActive), {
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
      previewPlaybackSource: 'timeline',
      previewPlaybackState: 'playing',
      selectedSecond: 35,
    })

    const video = createVideoStub({ currentTime: 30 })
    const videoRef = { current: video }
    renderHook(() => useVideoPreview([videoRef, { current: createVideoStub() }], true))

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

    renderHook(() => useVideoPreview([videoRef, { current: createVideoStub() }], true))

    await waitFor(() => {
      expect(useStore.getState().previewPlaybackState).toBe('paused')
    })

    expect(useStore.getState().previewPlaybackSource).toBe('video')
    expect(useStore.getState().selectedSecond).toBe(8)
  })

  test('hands the video element to the clip under the playhead', async () => {
    useStore.setState(
      videoStoreState(
        [
          createTestVideo({ id: 'video-1', path: 'C:\\clips\\first.mp4', durationSeconds: 10 }),
          createTestVideo({ id: 'video-2', path: 'C:\\clips\\second.mp4', durationSeconds: 10, importOrder: 1 }),
        ],
        { 'video-1': 0, 'video-2': 20 },
      ),
    )
    act(() => useStore.getState().setSelectedSecond(24))

    const firstElement = createVideoStub({ duration: 10 })
    const secondElement = createVideoStub({ duration: 10 })
    const { result } = renderHook(() => useVideoPreview([{ current: firstElement }, { current: secondElement }], true))

    expect(result.current.videoSrc).toBe('http://127.0.0.1:4321/video/import-video-2')
    expect(result.current.importId).toBe('import-video-2')
    expect(result.current.isOutOfRange).toBe(false)
    expect(result.current.slots).toEqual([
      { src: 'http://127.0.0.1:4321/video/import-video-2', isActive: true },
      { src: '', isActive: false },
    ])
    await waitFor(() => expect(firstElement.currentTime).toBe(4))

    // The first clip loads into the other element; the second clip stays parked as the next piece.
    act(() => useStore.getState().setSelectedSecond(3))

    expect(result.current.videoSrc).toBe('http://127.0.0.1:4321/video/import-video-1')
    expect(result.current.importId).toBe('import-video-1')
    expect(result.current.isOutOfRange).toBe(false)
    expect(result.current.slots).toEqual([
      { src: 'http://127.0.0.1:4321/video/import-video-2', isActive: false },
      { src: 'http://127.0.0.1:4321/video/import-video-1', isActive: true },
    ])
    await waitFor(() => expect(secondElement.currentTime).toBe(3))
    expect(firstElement.currentTime).toBe(0)

    act(() => useStore.getState().setSelectedSecond(15))

    expect(result.current.importId).toBe('import-video-2')
    expect(result.current.isOutOfRange).toBe(true)
    expect(result.current.slots.map((slot) => slot.isActive)).toEqual([true, false])
  })

  function lastClock() {
    return vi.mocked(useVideoPlaybackClock).mock.calls.at(-1)[0]
  }

  function playingAt(second) {
    useStore.setState({ previewPlaybackSource: 'video', previewPlaybackState: 'playing', selectedSecond: second })
  }

  test('continues with the next piece at its start when a piece ends, skipping the gap', () => {
    useStore.setState(
      videoStoreState(
        [
          createTestVideo({ id: 'video-1', path: 'C:\\clips\\first.mp4', durationSeconds: 10 }),
          createTestVideo({ id: 'video-2', path: 'C:\\clips\\second.mp4', durationSeconds: 10, importOrder: 1 }),
        ],
        { 'video-1': 0, 'video-2': 20 },
      ),
    )
    playingAt(9)
    const playing = createVideoStub({ duration: 10, paused: false, currentTime: 9 })
    const parked = createVideoStub({ duration: 10, currentTime: 3 })
    const slotRefs = [{ current: playing }, { current: parked }]
    const { result } = renderHook(() => useVideoPreview(slotRefs, true))
    expect(lastClock()).toMatchObject({ isActive: true, pieceEndSourceSecond: 10, videoOffsetSeconds: 0, videoRef: slotRefs[0] })
    expect(parked.currentTime).toBe(0)
    expect(result.current.slots.map((slot) => slot.src)).toEqual([
      'http://127.0.0.1:4321/video/import-video-1',
      'http://127.0.0.1:4321/video/import-video-2',
    ])

    act(() => lastClock().onPlaybackEnded())

    expect(useStore.getState()).toMatchObject({ previewPlaybackState: 'playing', previewPlaybackSource: 'video', selectedSecond: 20 })
    expect(lastClock()).toMatchObject({ pieceEndSourceSecond: 10, videoOffsetSeconds: 20, videoRef: slotRefs[1] })
    expect(result.current.slots.map((slot) => slot.isActive)).toEqual([false, true])
    expect(parked.play).toHaveBeenCalled()
    expect(playing.pause).toHaveBeenCalled()
  })

  test('jumps over a deleted part within the same video by swapping to the element parked after it', () => {
    const video = createTestVideo({ id: 'video-1', path: 'C:\\clips\\ride.mp4', durationSeconds: 30 })
    const state = videoStoreState([video], { 'video-1': 5 })
    state.videoEdits['video-1'].deletedParts = [{ id: 'cut', fromSecond: 10, toSecond: 20, transition: null }]
    useStore.setState(state)
    playingAt(14.9)
    const element = createVideoStub({ duration: 30, paused: false, currentTime: 9.9 })
    const twin = createVideoStub({ duration: 30 })
    const slotRefs = [{ current: element }, { current: twin }]
    renderHook(() => useVideoPreview(slotRefs, true))
    expect(lastClock()).toMatchObject({ pieceEndSourceSecond: 10 })
    expect(twin.currentTime).toBe(20)

    act(() => lastClock().onPlaybackEnded())

    expect(useStore.getState().selectedSecond).toBe(25)
    expect(lastClock()).toMatchObject({ pieceEndSourceSecond: 30, videoRef: slotRefs[1] })
    expect(twin.play).toHaveBeenCalled()
    expect(element.pause).toHaveBeenCalled()
  })

  test('after the last piece hands the activity to the timeline clock and holds the last frame', () => {
    playingAt(34.9)
    const video = createVideoStub({ duration: 30, paused: false, currentTime: 29.9 })
    renderHook(() => useVideoPreview([{ current: video }, { current: createVideoStub() }], true))

    act(() => lastClock().onPlaybackEnded())

    expect(video.pause).toHaveBeenCalled()
    expect(video.currentTime).toBeCloseTo(899 / 30, 9)
    expect(useStore.getState()).toMatchObject({ previewPlaybackState: 'playing', previewPlaybackSource: 'timeline', selectedSecond: 35 })
  })

  test('flags deleted parts and overlaps at the playhead', () => {
    const first = createTestVideo({ id: 'video-1', path: 'C:\\clips\\first.mp4', durationSeconds: 30 })
    const second = createTestVideo({ id: 'video-2', path: 'C:\\clips\\second.mp4', durationSeconds: 10, importOrder: 1 })
    const state = videoStoreState([first, second], { 'video-1': 0, 'video-2': 25 })
    state.videoEdits['video-1'].deletedParts = [{ id: 'cut', fromSecond: 5, toSecond: 10, transition: null }]
    useStore.setState({ ...state, selectedSecond: 7 })
    const { result } = renderHook(() => useVideoPreview([{ current: createVideoStub() }, { current: createVideoStub() }], true))
    expect(result.current).toMatchObject({ isDeleted: true, isInOverlap: false, isOutOfRange: true })

    act(() => useStore.getState().setSelectedSecond(27))
    expect(result.current).toMatchObject({ isDeleted: false, isInOverlap: true, isOutOfRange: false, importId: 'import-video-1' })

    act(() => useStore.getState().setSelectedSecond(31))
    expect(result.current).toMatchObject({ isInOverlap: false, importId: 'import-video-2' })
  })
})
