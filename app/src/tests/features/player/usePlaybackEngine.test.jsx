/**
 * Integration tests for playback-engine orchestration.
 *
 * The player should preserve the same user-visible rules while the internals
 * are split apart: video owns playback inside its window, timeline owns the
 * rest, and timeline playback still advances and stops on the exact frame.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

vi.mock('@/features/overlay-editor', () => ({
  getEffectivePreviewFps: vi.fn(() => 10),
}))

import usePlaybackEngine from '@/features/player/hooks/usePlaybackEngine'

function createOptions(overrides = {}) {
  return {
    activitySummary: {
      durationSeconds: 8,
    },
    backgroundMode: 'black',
    beginPreviewScrub: vi.fn(),
    commitPreviewScrub: vi.fn(),
    fallbackDurationSeconds: 0,
    importedVideoDuration: 4,
    importedVideoPath: 'C:\\clips\\ride.mp4',
    pausePreviewPlayback: vi.fn(),
    previewPlaybackSource: 'timeline',
    previewPlaybackState: 'paused',
    sceneFps: 30,
    selectedSecond: 1,
    setSelectedSecond: vi.fn(),
    startPreviewPlayback: vi.fn(),
    updatePreviewScrub: vi.fn(),
    updateRate: 1,
    videoSyncOffsetSeconds: 2,
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
      result.current.handlePlay()
    })

    expect(options.startPreviewPlayback).toHaveBeenCalledWith({
      source: 'video',
      second: 3,
    })
  })

  test('hands playback back to the timeline when the playhead leaves the imported video range', () => {
    const options = createOptions({
      backgroundMode: 'video',
      previewPlaybackSource: 'video',
      previewPlaybackState: 'playing',
      selectedSecond: 6,
    })

    renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    expect(options.startPreviewPlayback).toHaveBeenCalledWith({
      source: 'timeline',
      second: 6,
    })
  })

  test('advances timeline playback on animation frames and pauses at the exact end', () => {
    const options = createOptions({
      activitySummary: {
        durationSeconds: 3,
      },
      importedVideoPath: null,
      selectedSecond: 1,
    })
    const raf = installRafHarness()
    const nowSpy = vi.spyOn(performance, 'now')
    nowSpy.mockReturnValue(1000)

    const { result, rerender } = renderHook((props) => usePlaybackEngine(props), {
      initialProps: options,
    })

    act(() => {
      result.current.handlePlay()
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
})
