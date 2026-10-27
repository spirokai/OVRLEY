import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import usePlaybackEngine from '@/features/player/hooks/usePlaybackEngine'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function videoTimeline(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  const { clips, program, bounds } = buildTimelineProgram({
    videos: importedVideos,
    videoEdits,
    dragPreview: null,
    transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
  })
  return { program, videoBounds: bounds, videoIdentity: clips.map((clip) => `${clip.id}:${clip.durationSeconds}`).join('|') }
}

function singleVideoTimeline({ durationSeconds = 4, offsetSeconds = 2 } = {}) {
  return videoTimeline([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds })], { 'video-1': offsetSeconds })
}

function createOptions(overrides = {}) {
  return {
    activityDurationSeconds: 8,
    backgroundMode: 'black',
    beginPreviewScrub: vi.fn(),
    commitPreviewScrub: vi.fn(),
    fallbackDurationSeconds: 0,
    hasActivityData: true,
    pausePreviewPlayback: vi.fn(),
    previewPlaybackSource: 'timeline',
    previewPlaybackState: 'paused',
    sceneFps: 30,
    selectedSecond: 1,
    setSelectedSecond: vi.fn(),
    startPreviewPlayback: vi.fn(),
    updatePreviewScrub: vi.fn(),
    updateRate: 1,
    ...singleVideoTimeline(),
    ...overrides,
  }
}

function installRafHarness() {
  let nextId = 1
  const callbacks = new Map()

  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    const id = nextId
    nextId += 1
    callbacks.set(id, callback)
    return id
  })

  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
    callbacks.delete(id)
  })

  return {
    fireNextFrame(now) {
      const nextEntry = callbacks.entries().next().value
      if (!nextEntry) {
        throw new Error('Expected a queued animation frame')
      }

      const [id, callback] = nextEntry
      callbacks.delete(id)
      callback(now)
    },
    pendingFrameCount() {
      return callbacks.size
    },
  }
}

describe('usePlaybackEngine', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  test('starts video-backed playback when play begins inside the imported video window', () => {
    const options = createOptions({
      backgroundMode: 'video',
      selectedSecond: 3,
    })

    const { result } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.play()
    })

    expect(options.startPreviewPlayback).toHaveBeenCalledWith({
      source: 'video',
      second: 3,
    })
  })

  test('starts at the negative video beginning once, then resumes from paused activity time zero', () => {
    const options = createOptions({
      backgroundMode: 'video',
      selectedSecond: 0,
      ...singleVideoTimeline({ durationSeconds: 10, offsetSeconds: -5 }),
    })
    const { result, rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    expect(options.setSelectedSecond).toHaveBeenCalledWith(-5)
    rerender({ ...options, selectedSecond: -5 })

    act(() => {
      result.current.play()
    })
    expect(options.startPreviewPlayback).toHaveBeenLastCalledWith({
      source: 'video',
      second: -5,
    })

    rerender({ ...options, selectedSecond: 0 })
    act(() => result.current.play())
    expect(options.startPreviewPlayback).toHaveBeenLastCalledWith({
      source: 'video',
      second: 0,
    })
  })

  test('leaves the video-to-timeline handoff to the video ended command', () => {
    const options = createOptions({
      backgroundMode: 'video',
      previewPlaybackSource: 'video',
      previewPlaybackState: 'playing',
      selectedSecond: 6,
    })

    renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    expect(options.startPreviewPlayback).not.toHaveBeenCalled()
  })

  test('advances timeline playback on animation frames and pauses at the exact end', () => {
    const options = createOptions({
      activityDurationSeconds: 3,
      selectedSecond: 1,
      ...videoTimeline([], {}),
    })
    const raf = installRafHarness()
    const nowSpy = vi.spyOn(performance, 'now')
    nowSpy.mockReturnValue(1000)

    const { result, rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.play()
    })

    expect(options.startPreviewPlayback).toHaveBeenCalledWith({
      source: 'timeline',
      second: 1,
    })

    rerender({
      ...options,
      previewPlaybackSource: 'timeline',
      previewPlaybackState: 'playing',
    })

    act(() => {
      raf.fireNextFrame(1500)
    })

    expect(options.setSelectedSecond).toHaveBeenCalledWith(1.5)

    act(() => {
      raf.fireNextFrame(3500)
    })

    expect(options.pausePreviewPlayback).toHaveBeenCalledWith(3)
  })

  test('coalesces scrub samples and publishes one preview update per RAF', () => {
    const raf = installRafHarness()
    const options = createOptions()
    const { result, rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.scrubTo(2.25)
      result.current.scrubTo(3.5)
      result.current.scrubTo(4.75)
    })

    expect(raf.pendingFrameCount()).toBe(1)
    expect(options.beginPreviewScrub).not.toHaveBeenCalled()

    act(() => {
      raf.fireNextFrame(1000)
    })

    expect(options.beginPreviewScrub).toHaveBeenCalledTimes(1)
    expect(options.beginPreviewScrub).toHaveBeenCalledWith(4.75)

    rerender({
      ...options,
      previewPlaybackState: 'scrubbing',
      selectedSecond: 4.75,
    })

    act(() => {
      result.current.scrubTo(5.25)
      result.current.scrubTo(6.5)
    })

    expect(raf.pendingFrameCount()).toBe(1)

    act(() => {
      raf.fireNextFrame(1016)
    })

    expect(options.updatePreviewScrub).toHaveBeenCalledTimes(1)
    expect(options.updatePreviewScrub).toHaveBeenCalledWith(6.5)
  })

  test('commits scrub immediately and cancels stale preview work', () => {
    const raf = installRafHarness()
    const options = createOptions()
    const { result } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.scrubTo(2.25)
      result.current.commitScrub(3.5)
    })

    expect(options.beginPreviewScrub).not.toHaveBeenCalled()
    expect(options.updatePreviewScrub).not.toHaveBeenCalled()
    expect(options.commitPreviewScrub).toHaveBeenCalledTimes(1)
    expect(options.commitPreviewScrub).toHaveBeenCalledWith(3.5)
    expect(raf.pendingFrameCount()).toBe(0)
  })

  test('jumpToEnd pauses at total duration without slider-shaped arguments', () => {
    const options = createOptions()
    const { result } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.jumpToEnd()
    })

    expect(options.pausePreviewPlayback).toHaveBeenCalledWith(8)
  })

  test('starts playback in a gap at the next clip and inside a clip at the playhead', () => {
    const timeline = videoTimeline(
      [
        createTestVideo({ id: 'first', durationSeconds: 4 }),
        createTestVideo({ id: 'second', path: '/videos/second.mp4', durationSeconds: 4, importOrder: 1 }),
      ],
      { first: 0, second: 10 },
    )
    const options = createOptions({ activityDurationSeconds: 20, backgroundMode: 'video', selectedSecond: 6, ...timeline })
    const { result, rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.play()
    })
    expect(options.startPreviewPlayback).toHaveBeenLastCalledWith({ source: 'video', second: 10 })

    rerender({ ...options, selectedSecond: 11 })
    act(() => {
      result.current.play()
    })
    expect(options.startPreviewPlayback).toHaveBeenLastCalledWith({ source: 'video', second: 11 })
  })

  test('moves a zero playhead to an earlier start when the video collection changes', () => {
    const first = createTestVideo({ id: 'first', durationSeconds: 10 })
    const options = createOptions({ selectedSecond: 0, ...videoTimeline([first], { first: 2 }) })
    const { rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    expect(options.setSelectedSecond).not.toHaveBeenCalled()

    const second = createTestVideo({ id: 'second', path: '/videos/second.mp4', durationSeconds: 10, importOrder: 1 })
    const extended = videoTimeline([first, second], { first: 2, second: -3 })
    rerender({ ...options, ...extended })

    expect(extended.videoBounds).toEqual({ startSecond: -3, endSecond: 12 })
    expect(options.setSelectedSecond).toHaveBeenCalledWith(-3)
  })

  test('plays video telemetry without an external activity across the clip bounds', () => {
    const options = createOptions({
      activityDurationSeconds: 0,
      fallbackDurationSeconds: 73,
      ...videoTimeline(
        [
          createTestVideo({ id: 'first', durationSeconds: 5 }),
          createTestVideo({ id: 'second', path: '/videos/second.mp4', durationSeconds: 5, importOrder: 1 }),
        ],
        { first: 0, second: 7 },
      ),
    })
    const { result } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    expect(result.current.hasActivity).toBe(true)
    expect(result.current.totalDuration).toBe(12)
    expect(result.current.timelineMinimum).toBe(0)
  })

  test('disables playback without activity data or video telemetry', () => {
    const { result } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: createOptions({ activityDurationSeconds: 0, hasActivityData: false }),
    })

    expect(result.current.hasActivity).toBe(false)
    expect(result.current.totalDuration).toBe(6)
  })
})
