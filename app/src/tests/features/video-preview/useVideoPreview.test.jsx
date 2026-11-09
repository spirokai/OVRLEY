/**
 * Regression tests for paused preview-video synchronization.
 *
 * Switching the canvas background from a non-video mode to video mode should
 * immediately seek the preview element to the current playhead, even while
 * playback stays paused.
 */

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useStore from '@/store/useStore'
import { useVideoPreview } from '@/features/video-preview/hooks/useVideoPreview'
import { useVideoPlaybackClock } from '@/features/video-preview/hooks/useVideoPlaybackClock'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { listTransitionProxyJobs } from '@/lib/video-program/transition-proxies'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, cutEdits, videoStoreState } from '@/tests/helpers/videoFixtures'

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
      // Hard cuts unless a test adds transitions.
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS, type: 'none' },
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

  test('jumps over an excluded segment within the same video by swapping to the element parked after it', () => {
    const video = createTestVideo({ id: 'video-1', path: 'C:\\clips\\ride.mp4', durationSeconds: 30 })
    const state = videoStoreState([video], { 'video-1': 5 })
    Object.assign(
      state.videoEdits['video-1'],
      cutEdits([
        { id: 'c1', frame: 300, excluded: true },
        { id: 'c2', frame: 600 },
      ]),
    )
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

  function fadeBetweenTwoVideos() {
    useStore.setState({
      ...videoStoreState(
        [
          createTestVideo({ id: 'video-1', path: 'C:\\clips\\first.mp4', durationSeconds: 10 }),
          createTestVideo({ id: 'video-2', path: 'C:\\clips\\second.mp4', durationSeconds: 10, importOrder: 1 }),
        ],
        { 'video-1': 0, 'video-2': 20 },
      ),
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS, type: 'fade', durationSeconds: 2 },
    })
    playingAt(8.5)
    const parked = createVideoStub({ duration: 10 })
    const slotRefs = [{ current: createVideoStub({ duration: 10, paused: false, currentTime: 8.5 }) }, { current: parked }]
    const { result } = renderHook(() => useVideoPreview(slotRefs, true))
    return { parked, result }
  }

  test('shows the proxy over a transition window and continues after the start it showed', () => {
    const { parked, result } = fadeBetweenTwoVideos()
    expect(result.current.transition).toEqual({ proxyStatus: 'queued', proxyUrl: null, elapsedSeconds: 0.5, isPlaying: true })

    const { importedVideos, videoEdits, transitionDefaults } = useStore.getState()
    const { clips, program } = buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults })
    const [job] = listTransitionProxyJobs(program, clips)
    act(() => useStore.getState().setTransitionProxy(job.key, { status: 'ready', importId: 'proxy', url: 'http://127.0.0.1:4321/video/proxy' }))
    expect(result.current.transition).toMatchObject({ proxyStatus: 'ready', proxyUrl: 'http://127.0.0.1:4321/video/proxy' })
    // The second video waits where playback continues: after the two seconds the fade showed.
    expect(parked.currentTime).toBe(2)

    act(() => lastClock().onPlaybackEnded())
    expect(useStore.getState().selectedSecond).toBe(22)
    expect(result.current.transition).toBeNull()
  })

  test('without a rendered proxy cuts hard and continues from the first frame of the next video', () => {
    const { parked, result } = fadeBetweenTwoVideos()
    expect(result.current.transition).toMatchObject({ proxyStatus: 'queued', proxyUrl: null })
    expect(parked.currentTime).toBe(0)

    act(() => lastClock().onPlaybackEnded())
    expect(useStore.getState().selectedSecond).toBe(20)
  })
})
