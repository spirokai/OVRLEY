import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useWantedFilmstripWindows, { FILMSTRIP_WINDOW_SETTLE_MS } from '@/features/player/hooks/useWantedFilmstripWindows'
import useStore from '@/store/useStore'

// A 2 h 16:9 video at 0 s; with 16 px rem, 48 px tall tiles are 85 px wide.
const clip = { path: '/v/long.mp4', missing: false, offsetSeconds: 0, durationSeconds: 7200, resolution: { width: 1920, height: 1080 } }

describe('useWantedFilmstripWindows', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    useStore.setState(useStore.getInitialState(), true)
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  test('publishes the windows of the view once it settles, and nothing for views it passes through', () => {
    const { rerender } = renderHook((view) => useWantedFilmstripWindows({ clips: [clip], widthPx: 1000, ...view }), {
      initialProps: { viewStart: 1500, viewEnd: 2500 },
    })
    act(() => vi.advanceTimersByTime(FILMSTRIP_WINDOW_SETTLE_MS - 1))
    // Zoomed further in before the view settled: level 3's windows are never published.
    rerender({ viewStart: 100, viewEnd: 200 })
    act(() => vi.advanceTimersByTime(FILMSTRIP_WINDOW_SETTLE_MS - 1))
    expect(useStore.getState().wantedFilmstripWindows).toEqual({})

    act(() => vi.advanceTimersByTime(1))
    expect(useStore.getState().wantedFilmstripWindows).toEqual({ '/v/long.mp4': [{ kind: 'zoom', level: 0, window: 0 }] })
  })

  test('a timeline that goes away clears its windows', () => {
    const { unmount } = renderHook(() => useWantedFilmstripWindows({ clips: [clip], widthPx: 1000, viewStart: 100, viewEnd: 200 }))
    act(() => vi.advanceTimersByTime(FILMSTRIP_WINDOW_SETTLE_MS))
    expect(Object.keys(useStore.getState().wantedFilmstripWindows)).toEqual(['/v/long.mp4'])

    unmount()

    expect(useStore.getState().wantedFilmstripWindows).toEqual({})
  })
})
